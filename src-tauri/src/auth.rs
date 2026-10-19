//! GitHub 登录（Device Flow）+ 系统凭据库会话管理。
//!
//! 方案：docs/plans/GitHub登录与云同步方案.md（§3.1 Device Flow、§3.3 凭据库、§4.2 命令）。
//!
//! 安全纪律（方案 §4.5）：
//! - token 只存系统凭据库（Windows 凭据管理器），永不进入前端、日志、配置文件
//! - 所有 GitHub 网络请求在 Rust 侧发出，前端只拿到登录名与头像
//! - 凭据写操作经全局锁串行化；登出全量清场（token/登录名/头像/Gist id 一起删）

use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------

/// GitHub OAuth App 的 client_id（公开值，可硬编码，无需 client secret）。
/// 来源：GitHub → Settings → Developer settings → OAuth Apps → Phoenix-DRX
/// （已勾选 Enable Device Flow，未勾选 Expire user access tokens）。
const CLIENT_ID: &str = "Ov23li6DmoD7NexIE3MG";

/// 凭据库服务名
const CRED_SERVICE: &str = "Phoenix-DRX";
/// 凭据条目：访问令牌
const CRED_TOKEN: &str = "github-token";
/// 凭据条目：登录名
const CRED_LOGIN: &str = "github-login";
/// 凭据条目：头像地址
const CRED_AVATAR: &str = "github-avatar";
/// 凭据条目：云同步 Gist id（sync 模块共用；登出统一清理）
pub const CRED_GIST_ID: &str = "sync-gist-id";

/// Device Flow 三端点
const DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
const TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const USER_URL: &str = "https://api.github.com/user";

/// 请求 scope：读写 Gist（云同步）+ 读取账号资料（显示登录名/头像）
const OAUTH_SCOPE: &str = "gist read:user";

/// GitHub 未给 interval 时的兜底轮询间隔（秒）
const DEFAULT_INTERVAL: u64 = 5;
/// 网络异常重试上限（线性退避：第 n 次等 n*5 秒）
const NET_RETRY_MAX: u32 = 8;
/// 拉取账号资料的重试次数
const USER_RETRY_MAX: u32 = 3;

// ---------------------------------------------------------------------------
// 内存状态
// ---------------------------------------------------------------------------

/// 已登录会话（非敏感字段缓存；token 每次调用时从凭据库现取）
#[derive(Clone)]
struct Session {
    login: String,
    avatar_url: Option<String>,
}

/// 进行中的 Device Flow（轮询任务消费；设备码由任务参数持有，此处只存取消标志）
struct DeviceFlow {
    cancelled: bool,
}

static SESSION: OnceLock<Mutex<Option<Session>>> = OnceLock::new();
static DEVICE_FLOW: OnceLock<Mutex<Option<DeviceFlow>>> = OnceLock::new();
/// 凭据写串行化锁（方案 §4.5）
static CRED_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn lock_session() -> MutexGuard<'static, Option<Session>> {
    SESSION
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|p| p.into_inner())
}

fn lock_flow() -> MutexGuard<'static, Option<DeviceFlow>> {
    DEVICE_FLOW
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|p| p.into_inner())
}

// ---------------------------------------------------------------------------
// 系统凭据库（Windows 凭据管理器）
// ---------------------------------------------------------------------------

pub fn cred_set(key: &str, value: &str) -> Result<(), String> {
    let guard = CRED_LOCK.get_or_init(|| Mutex::new(()));
    let _lock = guard.lock().unwrap_or_else(|p| p.into_inner());
    keyring::Entry::new(CRED_SERVICE, key)
        .and_then(|e| e.set_password(value))
        .map_err(|e| format!("凭据写入失败：{e}"))
}

pub fn cred_get(key: &str) -> Result<Option<String>, String> {
    match keyring::Entry::new(CRED_SERVICE, key).and_then(|e| e.get_password()) {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("凭据读取失败：{e}")),
    }
}

pub fn cred_delete(key: &str) -> Result<(), String> {
    let guard = CRED_LOCK.get_or_init(|| Mutex::new(()));
    let _lock = guard.lock().unwrap_or_else(|p| p.into_inner());
    match keyring::Entry::new(CRED_SERVICE, key).and_then(|e| e.delete_credential()) {
        Ok(()) => Ok(()),
        // 条目本就不存在 = 已是干净状态，视为成功
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("凭据删除失败：{e}")),
    }
}

// ---------------------------------------------------------------------------
// 会话存取（供本模块命令与 sync 模块使用）
// ---------------------------------------------------------------------------

/// 当前登录名（未登录返回 None）
pub fn current_login() -> Option<String> {
    lock_session().as_ref().map(|s| s.login.clone())
}

/// 访问令牌（供 sync 模块调 GitHub API；未登录报错）
pub fn access_token() -> Result<String, String> {
    cred_get(CRED_TOKEN)?.ok_or_else(|| "尚未登录 GitHub".to_string())
}

/// 写入三条凭据 + 更新内存会话（调用方保证三条要么都成功、要么整体失败回滚由登出兜底）
fn store_session(token: &str, login: &str, avatar: Option<&str>) -> Result<(), String> {
    cred_set(CRED_TOKEN, token)?;
    cred_set(CRED_LOGIN, login)?;
    if let Some(a) = avatar {
        cred_set(CRED_AVATAR, a)?;
    }
    *lock_session() = Some(Session {
        login: login.to_string(),
        avatar_url: avatar.map(str::to_string),
    });
    Ok(())
}

/// 启动时从凭据库恢复会话（setup 钩子调用）
pub fn restore() {
    let has_token = matches!(cred_get(CRED_TOKEN), Ok(Some(_)));
    if !has_token {
        return;
    }
    let login = cred_get(CRED_LOGIN).ok().flatten();
    let avatar = cred_get(CRED_AVATAR).ok().flatten();
    if let Some(login) = login {
        *lock_session() = Some(Session {
            login,
            avatar_url: avatar,
        });
    }
}

// ---------------------------------------------------------------------------
// HTTP
// ---------------------------------------------------------------------------

/// GitHub API 要求所有请求带 User-Agent
pub(crate) fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("Phoenix-DRX")
        .build()
        .map_err(|e| format!("网络客户端初始化失败：{e}"))
}

// ---------------------------------------------------------------------------
// Tauri 命令
// ---------------------------------------------------------------------------

/// 登录状态（前端 auth_is_configured 的返回；字段名以 serde camelCase 输出为准）
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatus {
    pub configured: bool,
    pub login: Option<String>,
    pub avatar_url: Option<String>,
}

#[tauri::command]
pub fn auth_is_configured() -> AuthStatus {
    match lock_session().as_ref() {
        Some(s) => AuthStatus {
            configured: true,
            login: Some(s.login.clone()),
            avatar_url: s.avatar_url.clone(),
        },
        None => AuthStatus {
            configured: false,
            login: None,
            avatar_url: None,
        },
    }
}

/// 设备码请求响应（Accept: application/json）
#[derive(Deserialize)]
struct GhDeviceCodeResp {
    device_code: Option<String>,
    user_code: Option<String>,
    verification_uri: Option<String>,
    expires_in: Option<u64>,
    interval: Option<u64>,
    error: Option<String>,
    error_description: Option<String>,
}

/// auth_device_start 的返回（camelCase：userCode / verificationUri / expiresIn / interval）
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCodeStart {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[tauri::command]
pub async fn auth_device_start(app: AppHandle) -> Result<DeviceCodeStart, String> {
    if lock_flow().is_some() {
        return Err("已有登录流程进行中，请先取消".to_string());
    }
    if lock_session().is_some() {
        return Err("已经登录，无需重复登录".to_string());
    }

    let cli = client()?;
    let resp: GhDeviceCodeResp = cli
        .post(DEVICE_CODE_URL)
        .header("Accept", "application/json")
        .form(&[("client_id", CLIENT_ID), ("scope", OAUTH_SCOPE)])
        .send()
        .await
        .map_err(|e| format!("无法连接 GitHub：{e}"))?
        .json()
        .await
        .map_err(|e| format!("GitHub 响应解析失败：{e}"))?;

    // client_id 无效、App 未开 Device Flow 等在此暴露
    if let Some(err) = resp.error {
        return Err(resp
            .error_description
            .unwrap_or_else(|| format!("GitHub 返回错误：{err}")));
    }
    let device_code = resp
        .device_code
        .ok_or("GitHub 响应缺少设备码，请检查 OAuth App 是否勾选了设备授权")?;
    let user_code = resp
        .user_code
        .ok_or("GitHub 响应缺少配对码")?;
    let verification_uri = resp
        .verification_uri
        .unwrap_or_else(|| "https://github.com/login/device".to_string());
    let expires_in = resp.expires_in.unwrap_or(900);
    let interval = resp.interval.unwrap_or(DEFAULT_INTERVAL).max(1);

    *lock_flow() = Some(DeviceFlow { cancelled: false });

    // 轮询任务独立于命令返回：前端拿配对码先渲染，授权结果走事件
    tauri::async_runtime::spawn(poll_token(app, device_code, interval, expires_in));

    Ok(DeviceCodeStart {
        user_code,
        verification_uri,
        expires_in,
        interval,
    })
}

/// token 轮询响应
#[derive(Deserialize)]
struct GhTokenResp {
    access_token: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
    #[allow(dead_code)]
    interval: Option<u64>,
}

/// 账号资料响应
#[derive(Deserialize)]
struct GhUserResp {
    login: String,
    avatar_url: Option<String>,
}

/// Device Flow 轮询主循环：
/// - authorization_pending → 按 interval 续等；slow_down → 间隔 +5s
/// - expired_token / access_denied → 报错收场
/// - 网络异常线性退避重试（第 n 次等 n*5s），超上限报错收场
/// - 拿到 token → 拉 /user → 存凭据库 → 发成功事件
async fn poll_token(app: AppHandle, device_code: String, mut interval: u64, expires_in: u64) {
    let cli = match client() {
        Ok(c) => c,
        Err(e) => {
            *lock_flow() = None;
            emit_device_error(&app, "network", &e);
            return;
        }
    };
    let deadline = Instant::now() + Duration::from_secs(expires_in);
    let mut net_retries: u32 = 0;

    loop {
        // 取消/收场检查
        let cancelled = match lock_flow().as_ref() {
            Some(f) => f.cancelled,
            None => return, // 流已被清（登出兜底等），静默退出
        };
        if cancelled {
            *lock_flow() = None;
            return; // 用户主动取消：不发错误事件
        }
        if Instant::now() >= deadline {
            *lock_flow() = None;
            emit_device_error(&app, "expired", "配对码已过期，请重新登录");
            return;
        }

        tokio::time::sleep(Duration::from_secs(interval)).await;

        let resp = cli
            .post(TOKEN_URL)
            .header("Accept", "application/json")
            .form(&[
                ("client_id", CLIENT_ID),
                ("device_code", device_code.as_str()),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()
            .await;

        let parsed = match resp {
            Ok(r) => match r.json::<GhTokenResp>().await {
                Ok(v) => Some(v),
                Err(e) => {
                    net_retries += 1;
                    if net_retries >= NET_RETRY_MAX {
                        *lock_flow() = None;
                        emit_device_error(&app, "network", &format!("网络连接失败，已重试多次：{e}"));
                        return;
                    }
                    tokio::time::sleep(Duration::from_secs(u64::from(net_retries) * 5)).await;
                    continue;
                }
            },
            Err(e) => {
                net_retries += 1;
                if net_retries >= NET_RETRY_MAX {
                    *lock_flow() = None;
                    emit_device_error(&app, "network", &format!("网络连接失败，已重试多次：{e}"));
                    return;
                }
                tokio::time::sleep(Duration::from_secs(u64::from(net_retries) * 5)).await;
                continue;
            }
        };

        let resp = parsed.expect("parsed 仅在成功解析时为 Some");
        match resp.error.as_deref() {
            // 用户还没在 GitHub 页面输码确认：继续等
            Some("authorization_pending") => {
                net_retries = 0;
                continue;
            }
            // GitHub 要求放慢：间隔 +5s
            Some("slow_down") => {
                net_retries = 0;
                interval += 5;
                continue;
            }
            Some("expired_token") => {
                *lock_flow() = None;
                emit_device_error(&app, "expired", "配对码已过期，请重新登录");
                return;
            }
            Some("access_denied") => {
                *lock_flow() = None;
                emit_device_error(&app, "denied", "你在 GitHub 页面选择了拒绝授权");
                return;
            }
            Some(other) => {
                *lock_flow() = None;
                emit_device_error(
                    &app,
                    other,
                    &resp.error_description.unwrap_or_else(|| "GitHub 授权失败".to_string()),
                );
                return;
            }
            None => {}
        }

        // 无 error = 授权成功，拿 token 去拉账号资料
        if let Some(token) = resp.access_token {
            finish_login(app, cli, token).await;
            return;
        }
    }
}

/// 拉取账号资料并落库、发事件；/user 失败重试数次后整体失败（不留半登录态）
async fn finish_login(app: AppHandle, cli: reqwest::Client, token: String) {
    let mut user: Option<GhUserResp> = None;
    let mut last_err = String::new();
    for _ in 0..USER_RETRY_MAX {
        match cli
            .get(USER_URL)
            .header("Accept", "application/vnd.github+json")
            .bearer_auth(&token)
            .send()
            .await
        {
            Ok(r) => match r.json::<GhUserResp>().await {
                Ok(u) => {
                    user = Some(u);
                    break;
                }
                Err(e) => last_err = format!("GitHub 资料解析失败：{e}"),
            },
            Err(e) => last_err = format!("网络连接失败：{e}"),
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }

    *lock_flow() = None;
    let Some(u) = user else {
        emit_device_error(&app, "profile", &last_err);
        return;
    };

    if let Err(e) = store_session(&token, &u.login, u.avatar_url.as_deref()) {
        emit_device_error(&app, "store", &e);
        return;
    }

    let _ = app.emit(
        "auth:device-success",
        AuthStatus {
            configured: true,
            login: Some(u.login.clone()),
            avatar_url: u.avatar_url.clone(),
        },
    );
    emit_session_changed(&app);
}

// ---------------------------------------------------------------------------
// 取消 / 登出
// ---------------------------------------------------------------------------

/// 用户点「取消登录」：置取消标志，轮询任务下次醒来即静默退出
/// （async：Tauri 宏对同步 fn 返回 Result 的展开在 edition 2024 触发 never fallback）
#[tauri::command]
pub async fn auth_device_cancel() -> Result<(), String> {
    if let Some(f) = lock_flow().as_mut() {
        f.cancelled = true;
    }
    Ok(())
}

/// 登出：全量清场（三条凭据一起删 + 清内存会话 + 广播会话变更）。
/// 方案 §6.3：登出不推云端、不删 Gist，云端数据保留。
/// （async 原因同 auth_device_cancel：规避 never type fallback）
#[tauri::command]
pub async fn auth_logout(app: AppHandle) -> Result<(), String> {
    // 若有进行中的登录流程，一并取消
    if let Some(f) = lock_flow().as_mut() {
        f.cancelled = true;
    }
    let mut errs: Vec<String> = Vec::new();
    // 方案 §6.3：登出清凭据库（token + 登录名 + 头像 + Gist id）
    for key in [CRED_TOKEN, CRED_LOGIN, CRED_AVATAR, CRED_GIST_ID] {
        if let Err(e) = cred_delete(key) {
            errs.push(e);
        }
    }
    *lock_session() = None;
    emit_session_changed(&app);
    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs.join("；"))
    }
}

// ---------------------------------------------------------------------------
// 事件（方案 §4.3：auth:device-success / auth:device-error / auth:session-changed）
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceErrorPayload {
    code: String,
    message: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionChangedPayload {
    configured: bool,
    login: Option<String>,
}

fn emit_device_error(app: &AppHandle, code: &str, message: &str) {
    let _ = app.emit(
        "auth:device-error",
        DeviceErrorPayload {
            code: code.to_string(),
            message: message.to_string(),
        },
    );
}

/// 会话变更统一出口：读当前内存会话广播（登录成功/登出共用）
fn emit_session_changed(app: &AppHandle) {
    let (configured, login) = match lock_session().as_ref() {
        Some(s) => (true, Some(s.login.clone())),
        None => (false, None),
    };
    let _ = app.emit(
        "auth:session-changed",
        SessionChangedPayload { configured, login },
    );
}


