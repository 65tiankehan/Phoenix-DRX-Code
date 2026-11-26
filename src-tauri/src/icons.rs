//! 物品图标（来源 `https://assets.tarkov.dev/<tpl>-icon.webp`）。
//!
//! 为什么不用游戏文件：EFT 的 `StreamingAssets/.../assets` 里每个物品只有 LOD 网格与
//! 漫反射/gloss 贴图，**没有 2D 库存图标**（2026-11-25 用 UnityPy 拆过
//! `content/items/barter/item_barter_building_silicontube.bundle` 证实）；
//! `SPT_Data/images` 只有成就/手册/任务等分类图。图标只存在于社区 CDN。
//!
//! 三条硬约束：
//!   1. **永不阻塞界面** —— 先渲染首字色块，图标到货再替换（前端负责）；
//!   2. **离线可用** —— 命中缓存直接返回，不在网上等；
//!   3. **不爆盘** —— 缓存按「最多 5000 张 / 20 MB」封顶，超了按最久未改时间淘汰。
//!
//! 真档实测（2026-11-26）：`6af696c66f81eb33086665b1` 里出现 135 个 tpl，
//! 114 个有图（84%），合计 338 KB；缺图的是容器类（仓库/储藏处/Pockets 等）
//! 与 4.1 新物品（芳纶内衬、RGO 手榴弹）——这些一律回落到首字色块。

use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tauri::{AppHandle, Manager};

use crate::traders::base64_encode;

/// 单张图标上限（防止 CDN 返回异常大文件）
const MAX_ICON_BYTES: usize = 512 * 1024;
/// 缓存封顶：张数 / 总字节
const MAX_FILES: usize = 5000;
const MAX_TOTAL_BYTES: u64 = 20 * 1024 * 1024;
const FETCH_TIMEOUT_SECS: u64 = 8;

/// tpl 只允许 24 位十六进制，避免拼路径时被注入
fn is_valid_tpl(tpl: &str) -> bool {
    tpl.len() == 24 && tpl.bytes().all(|b| b.is_ascii_hexdigit())
}

fn cache_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("icons");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn data_url(bytes: &[u8]) -> String {
    format!("data:image/webp;base64,{}", base64_encode(bytes))
}

/// 读缓存：只返回本地已有的，不联网
#[tauri::command]
pub async fn icons_cached(app: AppHandle, tpls: Vec<String>) -> Result<HashMap<String, String>, String> {
    let dir = cache_dir(&app)?;
    let mut out = HashMap::new();
    for tpl in tpls {
        if !is_valid_tpl(&tpl) {
            continue;
        }
        let path = dir.join(format!("{}.webp", tpl));
        if let Ok(bytes) = fs::read(&path) {
            if !bytes.is_empty() && bytes.len() <= MAX_ICON_BYTES {
                out.insert(tpl, data_url(&bytes));
            }
        }
    }
    Ok(out)
}

/// 补图：本地没有的就按需下载，返回这批 tpl 里能拿到的图标
#[tauri::command]
pub async fn icons_ensure(app: AppHandle, tpls: Vec<String>) -> Result<HashMap<String, String>, String> {
    let dir = cache_dir(&app)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(FETCH_TIMEOUT_SECS))
        .build()
        .map_err(|e| e.to_string())?;

    let mut out = HashMap::new();
    let mut fetched = 0usize;

    for tpl in tpls {
        if !is_valid_tpl(&tpl) {
            continue;
        }
        let path = dir.join(format!("{}.webp", tpl));

        // 1) 缓存命中
        if let Ok(bytes) = fs::read(&path) {
            if !bytes.is_empty() && bytes.len() <= MAX_ICON_BYTES {
                out.insert(tpl.clone(), data_url(&bytes));
                continue;
            }
        }

        // 2) 下载（404 也算正常：容器类与新版本物品确实没图）
        let url = format!("https://assets.tarkov.dev/{}-icon.webp", tpl);
        let Ok(resp) = client.get(&url).send().await else {
            continue;
        };
        if !resp.status().is_success() {
            continue;
        }
        let Ok(bytes) = resp.bytes().await else {
            continue;
        };
        if bytes.is_empty() || bytes.len() > MAX_ICON_BYTES {
            continue;
        }
        if fs::write(&path, &bytes).is_ok() {
            out.insert(tpl.clone(), data_url(&bytes));
            fetched += 1;
        }
    }

    if fetched > 0 {
        let _ = prune(&dir);
    }
    Ok(out)
}

/// 缓存封顶：先按张数，再按总字节，超过就按修改时间从旧到新删
fn prune(dir: &PathBuf) -> std::io::Result<()> {
    let mut entries: Vec<(PathBuf, u64, std::time::SystemTime)> = Vec::new();
    let mut total: u64 = 0;
    for entry in fs::read_dir(dir)?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("webp") {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let size = meta.len();
        let time = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
        total += size;
        entries.push((path, size, time));
    }
    if entries.len() <= MAX_FILES && total <= MAX_TOTAL_BYTES {
        return Ok(());
    }
    entries.sort_by_key(|(_, _, t)| *t);
    let mut count = entries.len();
    for (path, size, _) in entries {
        if count <= MAX_FILES && total <= MAX_TOTAL_BYTES {
            break;
        }
        if fs::remove_file(&path).is_ok() {
            count -= 1;
            total = total.saturating_sub(size);
        }
    }
    Ok(())
}

/// 缓存统计（设置页显示用）
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IconCacheStat {
    pub files: usize,
    pub bytes: u64,
}

#[tauri::command]
pub async fn icons_stat(app: AppHandle) -> Result<IconCacheStat, String> {
    let dir = cache_dir(&app)?;
    let mut files = 0usize;
    let mut bytes = 0u64;
    for entry in fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten() {
        if entry.path().extension().and_then(|e| e.to_str()) != Some("webp") {
            continue;
        }
        if let Ok(meta) = entry.metadata() {
            files += 1;
            bytes += meta.len();
        }
    }
    Ok(IconCacheStat { files, bytes })
}
