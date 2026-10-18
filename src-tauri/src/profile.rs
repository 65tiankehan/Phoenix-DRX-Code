//! 存档服务 —— 当前服务器存档的枚举、首页数据、选择持久化与写回。
//! 存档目录：<服务器根>/SPT/user/profiles（与 ServerService 校验基准一致）。
//! 关键字段以真实服务器 profile 结构为准：characters.pmc.Info.{Nickname,Side,Level,GameVersion}、
//! characters.pmc.Inventory.items[]（货币按 _tpl 过滤求和，id 取自 SPT DefaultValues.cs）。
//! 选择状态持久化在 <appData>/state.json：{ current: Record<serverId, profileId> }。

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Manager};

use crate::backup::backup_before_write;
use crate::server::current_server;

/// 货币物品模板 id（SPT DefaultValues.cs 权威值）
const MONEY_RUB: &str = "5449016a4bdc2d6f028b456f";
const MONEY_USD: &str = "5696686a4bdc2da3298b456a";
const MONEY_EUR: &str = "569668774bdc2da2298b4568";

/// 单个存档元信息：id = 文件名去 .json
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileMeta {
    pub id: String,
    pub nickname: String,
    pub side: String,
    pub level: i64,
}

/// 首页概览卡数据：meta + 游戏版本 + 三币种金额（千分位预格式化）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileHero {
    pub id: String,
    pub nickname: String,
    pub side: String,
    pub level: i64,
    pub game_version: String,
    pub rub: String,
    pub usd: String,
    pub eur: String,
}

/// 保存结果：本次写入落下的备份文件路径（源缺失时为 None）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveResult {
    pub backup_path: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ProfileState {
    #[serde(default)]
    current: HashMap<String, String>,
}

/// 存档 id 仅允许安全文件名字符，防路径穿越
pub(crate) fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
}

pub(crate) fn profiles_dir(app: &AppHandle) -> Option<String> {
    Some(format!(
        "{}\\SPT\\user\\profiles",
        current_server(app)?.path.trim_end_matches(['\\', '/'])
    ))
}

fn state_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("state.json"))
}

fn load_state(app: &AppHandle) -> ProfileState {
    let Ok(path) = state_path(app) else {
        return ProfileState::default();
    };
    serde_json::from_str(&fs::read_to_string(path).unwrap_or_default()).unwrap_or_default()
}

fn save_state(app: &AppHandle, state: &ProfileState) -> Result<(), String> {
    let path = state_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

/// 服务器维度隔离状态：以 config.json 的 currentServerId 为键
fn server_key(app: &AppHandle) -> String {
    current_server(app).map(|s| s.id).unwrap_or_default()
}

fn read_raw_profile(path: &Path) -> Option<Value> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

/// 存档元信息；解析失败回退以 id 作名字
fn meta_of(id: &str, raw: Option<&Value>) -> ProfileMeta {
    let info = raw
        .and_then(|r| r.get("characters"))
        .and_then(|c| c.get("pmc"))
        .and_then(|p| p.get("Info"));
    ProfileMeta {
        id: id.to_string(),
        nickname: info
            .and_then(|i| i.get("Nickname"))
            .and_then(|v| v.as_str())
            .unwrap_or(id)
            .to_string(),
        side: info
            .and_then(|i| i.get("Side"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        level: info
            .and_then(|i| i.get("Level"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
    }
}

/// 货币堆叠求和：items 中 _tpl 匹配项的 upd.StackObjectsCount 之和（缺失按 0）
fn sum_money(items: &[Value], tpl: &str) -> i64 {
    items
        .iter()
        .filter(|i| i.get("_tpl").and_then(|v| v.as_str()) == Some(tpl))
        .map(|i| {
            i.get("upd")
                .and_then(|u| u.get("StackObjectsCount"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0)
        })
        .sum()
}

/// 千分位格式化（等价 JS toLocaleString('en-US')）
fn thousands(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    if n < 0 {
        format!("-{}", out)
    } else {
        out
    }
}

/// 列举当前服务器 profiles 目录下的存档（解析失败的文件回退 id 作名，仍可入选）
pub fn list_profiles(app: &AppHandle) -> Vec<ProfileMeta> {
    let Some(dir) = profiles_dir(app) else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
        // ".json" 为 5 字节 ASCII，去尾安全
        .filter(|f| f.to_lowercase().ends_with(".json") && is_safe_id(&f[..f.len() - 5]))
        .map(|f| f[..f.len() - 5].to_string())
        .collect();
    ids.sort();
    ids.iter()
        .map(|id| {
            let raw = read_raw_profile(&Path::new(&dir).join(format!("{}.json", id)));
            meta_of(id, raw.as_ref())
        })
        .collect()
}

/// 首页概览卡数据。档案缺失或不可读时返回 None
pub fn load_hero(app: &AppHandle, id: &str) -> Option<ProfileHero> {
    if !is_safe_id(id) {
        return None;
    }
    let dir = profiles_dir(app)?;
    let raw = read_raw_profile(&Path::new(&dir).join(format!("{}.json", id)))?;
    let items: Vec<Value> = raw
        .get("characters")
        .and_then(|c| c.get("pmc"))
        .and_then(|p| p.get("Inventory"))
        .and_then(|i| i.get("items"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let meta = meta_of(id, Some(&raw));
    let game_version = raw
        .get("characters")
        .and_then(|c| c.get("pmc"))
        .and_then(|p| p.get("Info"))
        .and_then(|i| i.get("GameVersion"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Some(ProfileHero {
        id: meta.id,
        nickname: meta.nickname,
        side: meta.side,
        level: meta.level,
        game_version,
        rub: thousands(sum_money(&items, MONEY_RUB)),
        usd: thousands(sum_money(&items, MONEY_USD)),
        eur: thousands(sum_money(&items, MONEY_EUR)),
    })
}

/// 当前服务器选中的存档 id；未选择时为 None
pub fn get_current_profile_id(app: &AppHandle) -> Option<String> {
    load_state(app).current.get(&server_key(app)).cloned()
}

/// 设置当前服务器的选中存档（只记 id，不校验存在性）
pub fn set_current_profile(app: &AppHandle, id: &str) -> Result<(), String> {
    let mut state = load_state(app);
    state.current.insert(server_key(app), id.to_string());
    save_state(app, &state)?;
    Ok(())
}

/// 保存写回：先强制备份（领域铁律：备份先行），再原字节写回。返回备份文件路径
pub fn save_profile(app: &AppHandle, id: &str) -> Result<SaveResult, String> {
    if !is_safe_id(id) {
        return Err(format!("Invalid profile id: {}", id));
    }
    let dir = profiles_dir(app).ok_or_else(|| "未选择 SPT 服务器".to_string())?;
    let path = Path::new(&dir).join(format!("{}.json", id));
    let bytes = fs::read(&path).map_err(|e| e.to_string())?;
    let backup_path = backup_before_write(app, &path)?;
    fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(SaveResult {
        backup_path: backup_path.map(|p| p.to_string_lossy().to_string()),
    })
}
