//! 备份服务 —— 存档写入前自动备份（领域铁律：备份先行）。
//! 备份目录：<appData>/backups/<profileKey>；文件名为 ISO 时间戳（`:`/`.` 换 `-`）。
//! 每个存档保留最新 MAX_BACKUPS 份，按文件名字典序清理（ISO 时间戳天然有序）。

use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use sha1::{Digest, Sha1};
use tauri::Manager;

const MAX_BACKUPS: usize = 10;

/// 备份键：文件主名 + 小写完整路径的 sha1 前 12 位（与旧版语义一致）
fn profile_key(profile_path: &Path) -> Result<String, String> {
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
