//! Scav 装备 / 口袋页（11-scav）数据服务 —— 与 09 仓库同构，数据源换 characters.scav。
//! - 装备槽：characters.scav.Inventory.items 中 parentId == Inventory.equipment 按
//!   slotId 定位（固定 12 槽 + 口袋特殊槽），行构造复用 stash::item_row（本地化名 /
//!   modded / is_weapon / is_container 口径与 PMC 一致）；
//! - 口袋：容器 = parentId == equipment 且 slotId == "Pockets" 的条目，口袋物品
//!   parentId == 该容器 id；pocketsName 为容器本地化名（口袋子页只读展示）；
//! - 写回（先备份，领域铁律）：仅 RemoveItems 一种 op（行内删除 / 清空装备共用，
//!   含子孙物品递归清除，参考 FinalRemoveItems），指针换 /characters/scav。

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::AppHandle;

use crate::backup::backup_before_write;
use crate::profile::{is_safe_id, profiles_dir, SaveResult};
use crate::stash::{
    collect_subtree, item_row, locales_table, EquipSlotRow, ItemsDb, StashItemRow, EQUIP_SLOTS,
};

/// Scav 页全量数据（装备槽 + 口袋）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScavData {
    pub equipment: Vec<EquipSlotRow>,
    pub pockets: Vec<StashItemRow>,
    /// 口袋容器本地化名（口袋子页 vcard 只读展示）
    pub pockets_name: String,
}

/// 写回操作（Scav 只需删除一种：行内删除与清空装备共用）
#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum ScavOp {
    RemoveItems {
        ids: Vec<String>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScavSaveData {
    pub ops: Vec<ScavOp>,
}

/// Scav 页全量数据（档案缺失或不可读时为 None）
pub fn load_scav(app: &AppHandle, id: &str) -> Option<ScavData> {
    if !is_safe_id(id) {
        return None;
    }
    let dir = profiles_dir(app)?;
    let raw: Value =
        serde_json::from_str(&fs::read_to_string(Path::new(&dir).join(format!("{}.json", id))).ok()?)
            .ok()?;
    let scav = raw.pointer("/characters/scav")?;
    let inv = scav.get("Inventory");
    let items = inv.and_then(|i| i.get("items")).and_then(|i| i.as_array())?;
    let equip_id = inv
        .and_then(|i| i.get("equipment"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let lang = crate::server::data_language(app, Some(&raw));
    let names = locales_table(app, &lang);
    let db = ItemsDb::load(app, names.as_ref());

    let row_of = |it: &Value| -> Option<StashItemRow> {
        Some(item_row(
            it.get("_id")?.as_str()?,
            it.get("_tpl")?.as_str()?,
            Some(it),
            &db,
        ))
    };

    // 装备槽：固定槽序逐槽找 parentId == equipment 的物品
    let equipment: Vec<EquipSlotRow> = EQUIP_SLOTS
        .iter()
        .map(|slot| EquipSlotRow {
            slot: slot.to_string(),
            item: items
                .iter()
                .find(|it| {
                    it.get("parentId").and_then(|v| v.as_str()) == Some(equip_id)
                        && it.get("slotId").and_then(|v| v.as_str()) == Some(*slot)
                })
                .and_then(row_of),
        })
        .collect();

    // 口袋容器：parentId == equipment 且 slotId == "Pockets"
    let (pockets_id, pockets_name) = items
        .iter()
        .find(|it| {
            it.get("parentId").and_then(|v| v.as_str()) == Some(equip_id)
                && it.get("slotId").and_then(|v| v.as_str()) == Some("Pockets")
        })
        .map(|it| {
            (
                it.get("_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                it.get("_tpl")
                    .and_then(|v| v.as_str())
                    .map(|tpl| db.name(tpl))
                    .unwrap_or_default(),
            )
        })
        .unwrap_or_else(|| (String::new(), String::new()));

    // 口袋：parentId == Pockets 物品 id 的条目
    let pockets: Vec<StashItemRow> = items
        .iter()
        .filter(|it| it.get("parentId").and_then(|v| v.as_str()) == Some(pockets_id.as_str()))
        .filter_map(row_of)
        .collect();

    Some(ScavData {
        equipment,
        pockets,
        pockets_name,
    })
}

/// Scav 页写回：先强制备份（领域铁律），后逐条执行 op 列表（仅 RemoveItems）
pub fn save_scav(app: &AppHandle, id: &str, data: &ScavSaveData) -> Result<SaveResult, String> {
    if !is_safe_id(id) {
        return Err(format!("Invalid profile id: {}", id));
    }
    let dir = profiles_dir(app).ok_or_else(|| "未选择 SPT 服务器".to_string())?;
    let path = Path::new(&dir).join(format!("{}.json", id));
    let txt = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut raw: Value = serde_json::from_str(&txt).map_err(|e| e.to_string())?;
    let backup_path = backup_before_write(app, &path)?;

    for op in &data.ops {
        match op {
            ScavOp::RemoveItems { ids } => {
                let items = raw
                    .pointer_mut("/characters/scav/Inventory/items")
                    .and_then(|v| v.as_array_mut())
                    .ok_or("Inventory.items 缺失")?;
                let kill = {
                    let snapshot: Vec<Value> = items.clone();
                    collect_subtree(ids, &snapshot)
                };
                items.retain(|it| {
                    it.get("_id")
                        .and_then(|v| v.as_str())
                        .map(|x| !kill.contains(x))
                        .unwrap_or(true)
                });
            }
        }
    }

    let out = serde_json::to_string_pretty(&raw).map_err(|e| e.to_string())?;
    fs::write(&path, out).map_err(|e| e.to_string())?;
    Ok(SaveResult {
        backup_path: backup_path.map(|p| p.to_string_lossy().to_string()),
    })
}
