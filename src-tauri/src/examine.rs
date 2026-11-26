//! 一键全部检视（参考 SkiTles55 4.1.1 `Views\ExaminedItemsTab.xaml` 的
//! `tab_examined_items_examine_all_button`）。
//!
//! 数据通路：
//!   - 存档 `characters.pmc.Encyclopedia` 是 `{ tpl: bool }` 映射（真档实测 102 条，绝大多数 false）；
//!     `true` = 该物品已检视，跳蚤市场才会显示它
//!   - 候选全集 = `<database>/templates/items.json` 里 `_type != "Node"` 的全部 tpl（节点不是物品）
//!   - 写入只把缺的键补成 true，**不动已有的键、不动 value 为 false 的历史键**
//!     （避免把玩家"没看过"的状态误改，也让重复执行幂等）
//! 写入前一律走 backup_before_write（章程 §四 铁律）。

use serde::Serialize;
use serde_json::{Map, Value};
use std::fs;
use tauri::AppHandle;

use crate::backup::backup_before_write;
use crate::character::database_dir;
use crate::profile::get_current_profile_id;
use crate::profile::profiles_dir;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExamineData {
    /// 数据库里的物品总数（不含 Node 节点）
    pub total: usize,
    /// 存档里已标为 true 的条目数
    pub examined: usize,
    /// 还没标为 true 的条目数（= total − examined，最小 0）
    pub remaining: usize,
}

struct Target {
    path: std::path::PathBuf,
    root: Value,
}

fn profile_target(app: &AppHandle) -> Result<Target, String> {
    let id = get_current_profile_id(app).ok_or("未选择存档")?;
    let dir = profiles_dir(app).ok_or("未选择 SPT 服务器")?;
    let path = std::path::Path::new(&dir).join(format!("{}.json", id));
    let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let root: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(Target { path, root })
}

/// 数据库里全部物品 tpl（`_type == "Node"` 是分类节点，跳过）
fn item_tpls(app: &AppHandle) -> Result<Vec<String>, String> {
    let db = database_dir(app).ok_or("未找到服务器数据库目录")?;
    let path = std::path::Path::new(&db).join("templates").join("items.json");
    let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let map: Map<String, Value> = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(map.len());
    for (tpl, val) in map.iter() {
        let is_node = val.get("_type").and_then(|v| v.as_str()) == Some("Node");
        if !is_node {
            out.push(tpl.clone());
        }
    }
    Ok(out)
}

fn read_encyclopedia(root: &Value) -> Map<String, Value> {
    root.pointer("/characters/pmc/Encyclopedia")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default()
}

fn count_examined(enc: &Map<String, Value>) -> usize {
    enc.values()
        .filter(|v| v.as_bool() == Some(true))
        .count()
}

/// 读取：总数 / 已检视 / 剩余
#[tauri::command]
pub async fn examine_status(app: AppHandle) -> Result<ExamineData, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let target = profile_target(&app)?;
        let enc = read_encyclopedia(&target.root);
        let tpls = item_tpls(&app)?;
        let examined = count_examined(&enc);
        let total = tpls.len();
        Ok(ExamineData {
            total,
            examined,
            remaining: total.saturating_sub(examined),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 写入：把所有缺失的物品 tpl 补成 true，返回备份路径
#[tauri::command]
pub async fn examine_all(app: AppHandle) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut target = profile_target(&app)?;
        let tpls = item_tpls(&app)?;

        let mut enc = read_encyclopedia(&target.root);
        let before = count_examined(&enc);
        for tpl in &tpls {
            // 已有 false 的键保留原值？——不保留：false 就是"没检视"，一键检视就是要它变 true
            if enc.get(tpl).and_then(|v| v.as_bool()) != Some(true) {
                enc.insert(tpl.clone(), Value::Bool(true));
            }
        }
        let after = count_examined(&enc);
        if after == before {
            // 已经全检视：不写盘、不产生备份
            return Ok(None);
        }

        let Some(chars) = target
            .root
            .get_mut("characters")
            .and_then(|v| v.as_object_mut())
        else {
            return Err("存档缺少 characters".into());
        };
        let Some(pmc) = chars.get_mut("pmc").and_then(|v| v.as_object_mut()) else {
            return Err("存档缺少 characters.pmc".into());
        };
        pmc.insert("Encyclopedia".into(), Value::Object(enc));

        let backup = backup_before_write(&app, &target.path)?;
        let text = serde_json::to_string(&target.root).map_err(|e| e.to_string())?;
        fs::write(&target.path, text).map_err(|e| e.to_string())?;
        Ok(backup.map(|p| p.to_string_lossy().to_string()))
    })
    .await
    .map_err(|e| e.to_string())?
}
