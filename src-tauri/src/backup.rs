//! 备份服务 —— 存档写入前自动备份（领域铁律：备份先行）。
//! 备份目录：<appData>/backups/<profileKey>；文件名为 ISO 时间戳（`:`/`.` 换 `-`）。
//! 每个存档保留最新 MAX_BACKUPS 份，按文件名字典序清理（ISO 时间戳天然有序）。

use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::Serialize;
use sha1::{Digest, Sha1};
use tauri::Manager;

const MAX_BACKUPS: usize = 10;

/// 备份清单行（id = 文件名主名，供回档 / 删除定位；timeMs 供前端格式化日期列）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupRow {
    pub id: String,
    pub time_ms: i64,
}

/// 备份键：文件主名 + 小写完整路径的 sha1 前 12 位（与旧版语义一致）
pub(crate) fn profile_key(profile_path: &Path) -> Result<String, String> {
    let stem = profile_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "无法解析存档文件名".to_string())?;
    let lower = profile_path.to_string_lossy().to_lowercase();
    let mut hasher = Sha1::new();
    hasher.update(lower.as_bytes());
    let hex = format!("{:x}", hasher.finalize());
    Ok(format!("{}_{}", stem, &hex[..12]))
}

/// 写入前备份：源文件存在则完整字节复制到备份目录并清理旧份。
/// 返回备份文件路径；源不存在返回 None。失败抛错，由调用方阻断写入。
pub fn backup_before_write(
    app: &tauri::AppHandle,
    profile_path: &Path,
) -> Result<Option<PathBuf>, String> {
    if !profile_path.exists() {
        return Ok(None);
    }
    let key = profile_key(profile_path)?;
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("backups")
        .join(key);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    // 文件名 = ISO 时间戳（`:`/`.` → `-`），如 2026-10-17T09-30-00-123Z
    let stamp = Utc::now().format("%Y-%m-%dT%H-%M-%S-%3fZ");
    let dest = dir.join(format!("{}.json", stamp));
    fs::copy(profile_path, &dest).map_err(|e| e.to_string())?;
    trim_old_backups(&dir)?;
    Ok(Some(dest))
}

/// 按文件名字典序保留最新 MAX_BACKUPS 份
fn trim_old_backups(dir: &Path) -> Result<(), String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
        .collect();
    names.sort();
    while names.len() > MAX_BACKUPS {
        let oldest = names.remove(0);
        // 清理失败不阻断主流程（本次备份本体已落盘）
        let _ = fs::remove_file(dir.join(oldest));
    }
    Ok(())
}

// ———————— 备份页（14-backup）：清单 / 回档 / 删除 ————————

/// 备份 id 安全校验：只放行 ISO 时间戳文件名（防路径穿越），与 backup_before_write 命名一致
fn is_safe_backup_id(id: &str) -> bool {
    id.len() == "2026-10-17T09-30-00-123Z".len()
        && id.bytes().all(|b| {
            b.is_ascii_alphanumeric() || b == b'-' || b == b'T' || b == b'Z'
        })
}

/// 备份目录（key 不存在视为空目录语义）
fn backup_dir(app: &tauri::AppHandle, profile_path: &Path) -> Result<PathBuf, String> {
    let key = profile_key(profile_path)?;
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("backups")
        .join(key))
}

/// 备份清单：按时间倒序（文件名字典序倒序，ISO 时间戳天然有序）；目录缺失为空清单
pub fn backup_list(app: &tauri::AppHandle, profile_path: &Path) -> Result<Vec<BackupRow>, String> {
    let dir = backup_dir(app, profile_path)?;
    let mut names: Vec<String> = match fs::read_dir(&dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
            .filter(|n| n.ends_with(".json"))
            .collect(),
        // 目录不存在 = 尚无备份
        Err(_) => return Ok(Vec::new()),
    };
    names.sort();
    names.reverse();
    Ok(names
        .iter()
        .filter_map(|n| {
            let stem = n.strip_suffix(".json")?;
            if !is_safe_backup_id(stem) {
                return None;
            }
            // 文件名 → Unix 毫秒；命名不符的条目跳过
            let t = chrono::NaiveDateTime::parse_from_str(stem, "%Y-%m-%dT%H-%M-%S-%3fZ")
                .ok()?
                .and_utc()
                .timestamp_millis();
            Some(BackupRow {
                id: stem.to_string(),
                time_ms: t,
            })
        })
        .collect())
}

/// 回档：当前档案先备份（写前铁律）→ 用选中备份覆盖 → 消耗该备份。
/// 备份内容先入内存，防 trim 误删目标份；档案缺失时回档仍可用（以备份为准重建）
pub fn backup_restore(
    app: &tauri::AppHandle,
    profile_path: &Path,
    backup_id: &str,
) -> Result<(), String> {
    if !is_safe_backup_id(backup_id) {
        return Err("备份标识不合法".to_string());
    }
    let dir = backup_dir(app, profile_path)?;
    let src = dir.join(format!("{}.json", backup_id));
    let content = fs::read(&src).map_err(|e| e.to_string())?;
    // 覆盖前把当前态也留一份（源不存在时 backup_before_write 返回 None，跳过）
    backup_before_write(app, profile_path)?;
    fs::write(profile_path, &content).map_err(|e| e.to_string())?;
    fs::remove_file(&src).map_err(|e| e.to_string())?;
    Ok(())
}

/// 删除单份备份：仅移除该文件，不影响当前档案
pub fn backup_delete(
    app: &tauri::AppHandle,
    profile_path: &Path,
    backup_id: &str,
) -> Result<(), String> {
    if !is_safe_backup_id(backup_id) {
        return Err("备份标识不合法".to_string());
    }
    let dir = backup_dir(app, profile_path)?;
    fs::remove_file(dir.join(format!("{}.json", backup_id))).map_err(|e| e.to_string())
}
