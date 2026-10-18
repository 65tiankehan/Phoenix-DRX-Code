//! 服务器注册表服务 —— SPT 服务器根目录的登记、校验与当前选择。
//! 校验基准（参考 4.0.1 DefaultValues.cs）：需要 <root>/SPT/SPT.Server.exe 与
//! <root>/SPT/user/profiles。注册表持久化在 <appData>/config.json：
//! { servers: [{id, path}], currentServerId }。

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use tauri::{AppHandle, Manager};

/// 已登记的服务器条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerEntry {
    pub id: String,
    pub path: String,
}

/// 单台服务器的实时状态；check：ok 通过，exe / profiles 为缺失项代号（文案由渲染层按 i18n 映射）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatus {
    pub check: String,
    /// profiles 下 *.json 存档数；profiles 缺失或不可读时为 -1
    pub profile_count: i32,
    /// 显示名：根目录末段
    pub name: String,
}

/// 列表行 = 条目 + 实时状态（平铺，对齐旧 IPC 返回形状）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerRow {
    pub id: String,
    pub path: String,
    pub check: String,
    pub profile_count: i32,
    pub name: String,
}

/// 登记结果
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddResult {
    pub id: String,
    pub duplicate: bool,
    pub status: ServerStatus,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Config {
    #[serde(default)]
    servers: Vec<ServerEntry>,
    #[serde(default)]
    current_server_id: Option<String>,
    /// 应用级标志：启动页已看过（首启完整版 → 回访短版）
    #[serde(default)]
    splash_seen: bool,
    /// 应用级标志：引导页已完成（不再自动弹出）
    #[serde(default)]
    onboarded: bool,
}

fn load_config(app: &AppHandle) -> Config {
    let Ok(path) = app.path().app_data_dir() else {
        return Config::default();
    };
    serde_json::from_str(&fs::read_to_string(path.join("config.json")).unwrap_or_default())
        .unwrap_or_default()
}

fn save_config(app: &AppHandle, config: &Config) -> Result<(), String> {
    let path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("config.json");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

/// Windows 路径大小写与分隔符不敏感，统一规范化后作去重与 id 基准
fn normalize_path(p: &str) -> String {
    p.trim().replace('/', "\\").to_lowercase()
}

/// id = 规范化路径的 sha1 前 12 位
fn id_for(p: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(normalize_path(p).as_bytes());
    let hex = format!("{:x}", hasher.finalize());
    hex[..12].to_string()
}

/// profiles 下 *.json 存档数（仅统计普通文件，扩展名不区分大小写）；目录不可读为 -1
fn count_profiles(dir: &Path) -> i32 {
    let Ok(entries) = fs::read_dir(dir) else {
        return -1;
    };
    entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            let path = e.path();
            path.is_file()
                && path
                    .extension()
                    .and_then(|x| x.to_str())
                    .map(|x| x.eq_ignore_ascii_case("json"))
                    .unwrap_or(false)
        })
        .count() as i32
}

/// 校验服务器根目录并采集实时状态
pub fn check_server(root_path: &str) -> ServerStatus {
    let spt_dir = Path::new(root_path).join("SPT");
    let exe_ok = spt_dir.join("SPT.Server.exe").exists();
    let profiles_dir = spt_dir.join("user").join("profiles");
    let profiles_ok = profiles_dir.exists();

    let profile_count = if profiles_ok {
        count_profiles(&profiles_dir)
    } else {
        -1
    };

    let name = Path::new(root_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    ServerStatus {
        check: if exe_ok && profiles_ok {
            "ok"
        } else if exe_ok {
            "profiles"
        } else {
            "exe"
        }
        .to_string(),
        profile_count,
        name,
    }
}

fn row_of(entry: &ServerEntry) -> ServerRow {
    let status = check_server(&entry.path);
    ServerRow {
        id: entry.id.clone(),
        path: entry.path.clone(),
        check: status.check,
        profile_count: status.profile_count,
        name: status.name,
    }
}

pub fn list_servers(app: &AppHandle) -> Vec<ServerRow> {
    load_config(app).servers.iter().map(row_of).collect()
}

/// 登记服务器目录。无效路径不拒绝，只登记状态；重复路径不重复入库
pub fn add_server(app: &AppHandle, root_path: &str) -> Result<AddResult, String> {
    let id = id_for(root_path);
    let status = check_server(root_path);
    let mut config = load_config(app);
    if config.servers.iter().any(|s| s.id == id) {
        return Ok(AddResult {
            id,
            duplicate: true,
            status,
        });
    }

    config.servers.push(ServerEntry {
        id: id.clone(),
        path: root_path.to_string(),
    });
    save_config(app, &config)?;
    Ok(AddResult {
        id,
        duplicate: false,
        status,
    })
}

/// 移除登记（幂等）。移除的是当前选中项时，同时清空当前选择
pub fn remove_server(app: &AppHandle, id: &str) -> Result<(), String> {
    let mut config = load_config(app);
    config.servers.retain(|s| s.id != id);
    if config.current_server_id.as_deref() == Some(id) {
        config.current_server_id = None;
    }
    save_config(app, &config)?;
    Ok(())
}

/// 设为当前服务器。返回是否确有此登记
pub fn use_server(app: &AppHandle, id: &str) -> Result<bool, String> {
    let mut config = load_config(app);
    if !config.servers.iter().any(|s| s.id == id) {
        return Ok(false);
    }
    config.current_server_id = Some(id.to_string());
    save_config(app, &config)?;
    Ok(true)
}

/// 当前选中的服务器（含实时状态）；未选择或登记已失效时为 None
pub fn current_server(app: &AppHandle) -> Option<ServerRow> {
    let config = load_config(app);
    let current_id = config.current_server_id.as_ref()?;
    config.servers.iter().find(|s| &s.id == current_id).map(row_of)
}

/// 应用级标志快照（启动页/引导页首启编排用；与服务器登记同库 config.json）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppFlags {
    pub splash_seen: bool,
    pub onboarded: bool,
}

pub fn app_flags(app: &AppHandle) -> AppFlags {
    let c = load_config(app);
    AppFlags {
        splash_seen: c.splash_seen,
        onboarded: c.onboarded,
    }
}

/// 标记启动页已看过：下次启动走短版
pub fn mark_splash_seen(app: &AppHandle) -> Result<(), String> {
    let mut c = load_config(app);
    c.splash_seen = true;
    save_config(app, &c)
}

/// 标记引导页已完成：不再自动弹出
pub fn mark_onboarded(app: &AppHandle) -> Result<(), String> {
    let mut c = load_config(app);
    c.onboarded = true;
    save_config(app, &c)
}
