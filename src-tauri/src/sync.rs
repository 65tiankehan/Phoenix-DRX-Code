//! Gist 云同步（Rust 侧只做 Gist IO + 归属校验；合并引擎在前端 sync store）。
//!
//! 方案：docs/plans/GitHub登录与云同步方案.md（§3.2 Gist 结构、§5 云同步设计）。
//!
//! 职责边界（实现定案，区别于方案 §4.2 草案）：
//! - sync_pull：读远端 settings（发现/复用 Gist + 三元组校验），返回给前端
//! - sync_push：把前端传来的 settings 写入远端（有 id 直接 PATCH，无 id 走发现/创建）
//! - 「立即同步」不设独立命令：前端编排 pull → 合并 → 有脏键才 push
//! - 合并、脏标记、防回环闸门、防抖节流冷却全部在前端
//!
//! 归属校验三元组（§5.3）：owner.login == 当前用户 && 文件名匹配 && 描述含标记。
//! 不满足的 Gist 一律不碰（既不读也不写）。

use std::collections::HashMap;

use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::auth;

// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------

/// 云同步专用 Gist 里的单文件名（发现匹配键之一）
const GIST_FILENAME: &str = "phoenix-drx-sync.json";
/// Gist 描述标记（发现匹配键之一；描述就是这串字面量）
const GIST_DESC: &str = "Phoenix-DRX Sync Data";
/// Gists API 根
const GISTS_API: &str = "https://api.github.com/gists";
/// 云端 JSON 结构版本（§5.4：不认识的版本拒绝应用）
const SCHEMA_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// 云端数据结构（方案 §3.2）
// ---------------------------------------------------------------------------

/// settings 四键（Option：pull 时远端可能缺键；字段名以 serde camelCase 输出为准）
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSettings {
    pub sound_enabled: Option<bool>,
    pub motion_enabled: Option<bool>,
    pub locale: Option<String>,
    pub read_announcements: Option<Vec<String>>,
}

/// Gist 文件里的 JSON 载荷
#[derive(Serialize, Deserialize)]
struct SyncPayload {
    schema_version: u32,
    updated_at: String,
    settings: SyncSettings,
}

/// sync_pull 返回（前端合并引擎消费；未登录 / 远端无数据时 settings 为 None）
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullResult {
    pub settings: Option<SyncSettings>,
    pub updated_at: Option<String>,
}

// ---------------------------------------------------------------------------
// GitHub Gist API 响应结构（只反序列化需要的字段；GitHub 原生 snake_case）
// ---------------------------------------------------------------------------

#[derive(Clone, Deserialize)]
struct GhGist {
    id: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    owner: Option<GhOwner>,
    #[serde(default)]
    files: HashMap<String, GhGistFile>,
}

#[derive(Clone, Deserialize)]
struct GhOwner {
    login: Option<String>,
}

#[derive(Clone, Deserialize)]
struct GhGistFile {
    #[serde(default)]
    content: Option<String>,
}

/// 创建/更新 Gist 请求体
#[derive(Serialize)]
struct GistUpsertReq<'a> {
    description: &'a str,
    public: bool,
    files: HashMap<&'a str, GistFileUpsert<'a>>,
}

#[derive(Serialize)]
struct GistFileUpsert<'a> {
    content: &'a str,
}

// ---------------------------------------------------------------------------
// Gist id 存取（凭据库，登出统一清）
// ---------------------------------------------------------------------------

fn gist_id_get() -> Result<Option<String>, String> {
    auth::cred_get(auth::CRED_GIST_ID)
}

fn gist_id_set(id: &str) -> Result<(), String> {
    auth::cred_set(auth::CRED_GIST_ID, id)
}

fn gist_id_clear() -> Result<(), String> {
    auth::cred_delete(auth::CRED_GIST_ID)
}

// ---------------------------------------------------------------------------
// 归属校验（方案 §5.3 三元组）
// ---------------------------------------------------------------------------

/// 归属校验三元组：文件名匹配 + 描述含标记 + owner 是当前用户
fn is_owned(gist: &GhGist, login: &str) -> bool {
    gist.files.contains_key(GIST_FILENAME)
        && gist
            .description
            .as_deref()
            .map(|d| d.contains(GIST_DESC))
            .unwrap_or(false)
        && gist.owner.as_ref().and_then(|o| o.login.as_deref()) == Some(login)
}

// ---------------------------------------------------------------------------
// 事件（方案 §4.3：sync:done，成败均发；direction = "pull" / "push"）
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncDonePayload {
    ok: bool,
    direction: String,
}

fn emit_done(app: &AppHandle, ok: bool, direction: &str) {
    let _ = app.emit(
        "sync:done",
        SyncDonePayload {
            ok,
            direction: direction.to_string(),
        },
    );
}

// ---------------------------------------------------------------------------
// Gist IO
// ---------------------------------------------------------------------------

/// 单个 Gist 读取；404 返回 Ok(None)（远端已删，调用方清存 id）
async fn get_gist(
    cli: &reqwest::Client,
    token: &str,
    id: &str,
) -> Result<Option<GhGist>, String> {
    let url = format!("{GISTS_API}/{id}");
    let resp = cli
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| format!("无法连接 GitHub：{e}"))?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if let Err(e) = resp.error_for_status_ref() {
        return Err(format!("GitHub 返回错误：{e}"));
    }
    resp.json::<GhGist>()
        .await
        .map(Some)
        .map_err(|e| format!("GitHub 响应解析失败：{e}"))
}

/// 首登发现：翻 gists 列表（每页 100，最多 2 页；列表按最近更新排序），
/// 第一个满足三元组的就是当前用户最新的同步 Gist
async fn find_gist(
    cli: &reqwest::Client,
    token: &str,
    login: &str,
) -> Result<Option<GhGist>, String> {
    for page in 1..=2u32 {
        let url = format!("{GISTS_API}?per_page=100&page={page}");
        let resp = cli
            .get(&url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| format!("无法连接 GitHub：{e}"))?;
        if let Err(e) = resp.error_for_status_ref() {
            return Err(format!("GitHub 返回错误：{e}"));
        }
        let list: Vec<GhGist> = resp
            .json()
            .await
            .map_err(|e| format!("GitHub 响应解析失败：{e}"))?;
        let full_page = list.len() >= 100;
        if let Some(hit) = list.iter().find(|g| is_owned(g, login)) {
            return Ok(Some(hit.clone()));
        }
        if !full_page {
            break;
        }
    }
    Ok(None)
}

/// PATCH 已有 Gist（更新文件内容）；返回 Gist id
async fn patch_gist(
    cli: &reqwest::Client,
    token: &str,
    id: &str,
    body: &GistUpsertReq<'_>,
) -> Result<String, String> {
    let url = format!("{GISTS_API}/{id}");
    let resp = cli
        .patch(&url)
        .bearer_auth(token)
        .json(body)
        .send()
        .await
        .map_err(|e| format!("无法连接 GitHub：{e}"))?;
    if let Err(e) = resp.error_for_status_ref() {
        return Err(format!("GitHub 返回错误：{e}"));
    }
    let g: GhGist = resp
        .json()
        .await
        .map_err(|e| format!("GitHub 响应解析失败：{e}"))?;
    Ok(g.id)
}

/// POST 创建新 Gist；返回 Gist id
async fn create_gist(
    cli: &reqwest::Client,
    token: &str,
    body: &GistUpsertReq<'_>,
) -> Result<String, String> {
    let resp = cli
        .post(GISTS_API)
        .bearer_auth(token)
        .json(body)
        .send()
        .await
        .map_err(|e| format!("无法连接 GitHub：{e}"))?;
    if let Err(e) = resp.error_for_status_ref() {
        return Err(format!("GitHub 返回错误：{e}"));
    }
    let g: GhGist = resp
        .json()
        .await
        .map_err(|e| format!("GitHub 响应解析失败：{e}"))?;
    Ok(g.id)
}

/// 组装 upsert 请求体（私有 Gist + 固定描述 + 单文件内容）
fn upsert_body(content: &str) -> GistUpsertReq<'_> {
    let mut files = HashMap::new();
    files.insert(GIST_FILENAME, GistFileUpsert { content });
    GistUpsertReq {
        description: GIST_DESC,
        public: false,
        files,
    }
}

// ---------------------------------------------------------------------------
// Tauri 命令（方案 §4.2：sync_pull / sync_push；sync:done 成败均发）
// ---------------------------------------------------------------------------

/// 拉取远端 settings：有存 id 直接读（三元组校验，不符/404 清 id），
/// 无 id 走首登发现；远端无数据返回 settings = None（前端按首次使用处理）
async fn pull_inner() -> Result<PullResult, String> {
    let mut result = PullResult {
        settings: None,
        updated_at: None,
    };
    let token = auth::access_token()?;
    let Some(login) = auth::current_login() else {
        return Ok(result); // 未登录：返回空而不是报错（前端只在登录态调用，防御性收场）
    };
    let cli = auth::client()?;

    if let Some(id) = gist_id_get()? {
        match get_gist(&cli, &token, &id).await {
            Ok(Some(g)) if is_owned(&g, &login) => {
                fill_result(&g, &mut result);
            }
            // 归属不符或远端已删：清存 id，按无远端处理（下次 pull 走发现）
            Ok(_) => {
                gist_id_clear()?;
            }
            Err(e) => return Err(e),
        }
    } else if let Some(g) = find_gist(&cli, &token, &login).await? {
        gist_id_set(&g.id)?;
        fill_result(&g, &mut result);
    }
    Ok(result)
}

/// 从 Gist 文件里解出 settings 载荷填入结果（schemaVersion 不认识则拒收）
fn fill_result(g: &GhGist, result: &mut PullResult) {
    let Some(f) = g.files.get(GIST_FILENAME) else {
        return;
    };
    let Some(content) = f.content.as_deref() else {
        return;
    };
    let payload: SyncPayload = match serde_json::from_str(content) {
        Ok(p) => p,
        Err(_) => return, // 内容损坏：按无数据处理，不污染本地
    };
    if payload.schema_version != SCHEMA_VERSION {
        return; // 不认识的版本：拒绝应用（§5.4）
    }
    result.settings = Some(payload.settings);
    result.updated_at = payload.updated_at.into();
}

#[tauri::command]
pub async fn sync_pull(app: AppHandle) -> Result<PullResult, String> {
    let r = pull_inner().await;
    emit_done(&app, r.is_ok(), "pull");
    r
}

/// 推送 settings 到远端：有存 id 直接 PATCH（404 清 id 重建 / 不符重新发现），
/// 无 id 先发现再 PATCH 复用或 POST 创建。登出不推（§6.3），由前端保证调用时机。
async fn push_inner(settings: SyncSettings) -> Result<(), String> {
    let token = auth::access_token()?;
    let Some(login) = auth::current_login() else {
        return Err("未登录，无法同步".to_string());
    };
    let cli = auth::client()?;

    let payload = SyncPayload {
        schema_version: SCHEMA_VERSION,
        updated_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        settings,
    };
    let content = serde_json::to_string(&payload).map_err(|e| format!("同步数据序列化失败：{e}"))?;
    let body = upsert_body(&content);

    let id = match gist_id_get()? {
        Some(id) => match get_gist(&cli, &token, &id).await {
            Ok(Some(g)) if is_owned(&g, &login) => Some(id),
            // 归属不符：不碰别人的 Gist，走重新发现
            Ok(Some(_)) => {
                gist_id_clear()?;
                None
            }
            // 远端已删：清 id，下面走发现/创建重建
            Ok(None) => {
                gist_id_clear()?;
                None
            }
            Err(e) => return Err(e),
        },
        None => None,
    };

    match id {
        Some(id) => {
            patch_gist(&cli, &token, &id, &body).await?;
        }
        None => match find_gist(&cli, &token, &login).await? {
            Some(g) => {
                gist_id_set(&g.id)?;
                patch_gist(&cli, &token, &g.id, &body).await?;
            }
            None => {
                let new_id = create_gist(&cli, &token, &body).await?;
                gist_id_set(&new_id)?;
            }
        },
    }
    Ok(())
}

#[tauri::command]
pub async fn sync_push(app: AppHandle, settings: SyncSettings) -> Result<(), String> {
    let r = push_inner(settings).await;
    emit_done(&app, r.is_ok(), "push");
    r
}
