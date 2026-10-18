//! 公告拉取：Opendata 公开仓 announcement.json + killSwitch 远程急停 + 本地缓存回退
//!
//! 蓝本 tengri-novel announcement.ts：三镜像逐个尝试（raw → ghfast 反代 → jsDelivr），
//! 首个解析成功者胜出并写缓存；全挂回退缓存；killSwitch=true 时公告视为空。
//! 防御性解析：version 缺失即换下一镜像，形状不对的条目直接丢弃，
//! 不让 Opendata 仓改坏拖垮客户端。

use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager};

/// 单条公告（camelCase 与前端 PhxAnnouncement 对齐）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnouncementItem {
    pub id: String,
    pub title: String,
    pub body: String,
    pub action_label: Option<String>,
    pub action_url: Option<String>,
}

/// 拉取结果；source 标明数据来源（remote / cache / none），前端展示不按来源分支
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnouncementPayload {
    pub announcements: Vec<AnnouncementItem>,
    pub source: String,
}

/// Opendata announcement.json 顶层结构（只取需要的字段，其余忽略）
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPayload {
    /// version 缺失视为载荷损坏（对齐蓝本防御），换下一镜像
    version: Option<u32>,
    #[serde(default)]
    kill_switch: bool,
    #[serde(default)]
    announcements: Vec<serde_json::Value>,
}

const SOURCES: [&str; 3] = [
    "https://raw.githubusercontent.com/65tiankehan/Phoenix-DRX-Opendata/main/announcement.json",
    "https://ghfast.top/https://raw.githubusercontent.com/65tiankehan/Phoenix-DRX-Opendata/main/announcement.json",
    "https://cdn.jsdelivr.net/gh/65tiankehan/Phoenix-DRX-Opendata@main/announcement.json",
];

/// 缓存落 app_data_dir（与 settings.json 同目录模式）
const CACHE_FILE: &str = "announcement-cache.json";
const TIMEOUT: Duration = Duration::from_secs(10);

fn cache_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    Some(app.path().app_data_dir().ok()?.join(CACHE_FILE))
}

/// 防御性条目解析：id/title/body 缺一或为空即丢弃该条
fn parse_items(raw: &RawPayload) -> Vec<AnnouncementItem> {
    raw.announcements
        .iter()
        .filter_map(|v| {
            let id = v.get("id")?.as_str()?;
            let title = v.get("title")?.as_str()?;
            let body = v.get("body")?.as_str()?;
            if id.is_empty() || title.is_empty() || body.is_empty() {
                return None;
            }
            Some(AnnouncementItem {
                id: id.to_string(),
                title: title.to_string(),
                body: body.to_string(),
                action_label: v.get("actionLabel").and_then(|x| x.as_str()).map(str::to_string),
                action_url: v.get("actionUrl").and_then(|x| x.as_str()).map(str::to_string),
            })
        })
        .collect()
}

fn read_cache(app: &AppHandle) -> Option<AnnouncementPayload> {
    let text = std::fs::read_to_string(cache_path(app)?).ok()?;
    let raw: RawPayload = serde_json::from_str(&text).ok()?;
    // 缓存里的 killSwitch 同样生效（急停要能切断已缓存内容）
    let announcements = if raw.kill_switch {
        Vec::new()
    } else {
        parse_items(&raw)
    };
    Some(AnnouncementPayload {
        announcements,
        source: "cache".into(),
    })
}

fn write_cache(app: &AppHandle, text: &str) {
    if let Some(path) = cache_path(app) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, text);
    }
}

async fn fetch_text(client: &reqwest::Client, url: &str) -> Option<String> {
    client
        .get(url)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .text()
        .await
        .ok()
}

/// 业务入口：三镜像逐个尝试 → 全挂回退本地缓存 → 无缓存 source=none
pub async fn fetch_announcement(app: &AppHandle) -> AnnouncementPayload {
    let none = || AnnouncementPayload {
        announcements: Vec::new(),
        source: "none".into(),
    };
    let Ok(client) = reqwest::Client::builder().timeout(TIMEOUT).build() else {
        return read_cache(app).unwrap_or_else(none);
    };
    for url in SOURCES {
        let Some(text) = fetch_text(&client, url).await else {
            continue;
        };
        let Ok(raw) = serde_json::from_str::<RawPayload>(&text) else {
            continue;
        };
        if raw.version.is_none() {
            continue;
        }
        write_cache(app, &text);
        // killSwitch = 官方紧急停用：公告视为空（缓存照常刷新，解除后即恢复）
        if raw.kill_switch {
            return AnnouncementPayload {
                announcements: Vec::new(),
                source: "remote".into(),
            };
        }
        return AnnouncementPayload {
            announcements: parse_items(&raw),
            source: "remote".into(),
        };
    }
    read_cache(app).unwrap_or_else(none)
}
