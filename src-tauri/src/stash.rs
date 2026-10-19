//! 仓库页（09-stash）数据服务 —— 读全量 / 写回（仅 PMC；Scav 装备与口袋归 11 屏）。
//! 结构对照参考编辑器（CharacterInventory.cs / StashTabViewModel.cs / TarkovItem.cs）：
//! - 仓库表：Inventory.items 中 parentId == Inventory.stash 的直接子项，行含本地化名、
//!   数量（upd.StackObjectsCount，缺省 1）、modded 标记（tpl 不在服务器物品库，
//!   参考 IsInItemsDB）、is_weapon（_props.RecoilForceUp ≠ 0，参考 TarkovItem.IsWeapon）、
//!   is_container（_props.Grids 非空，参考 InventoryItem.IsContainer）；
//! - 货币：三货币 tpl 聚合 StackObjectsCount（参考 DefaultValues 的 Moneys*Tpl）；
//! - 附加行数：characters.pmc.StashRowsBonusCount（仓库尺寸行数加成）；
//! - 装备槽：parentId == Inventory.equipment 按 slotId 定位（固定 12 槽 + 口袋特殊槽，
//!   口袋内物品 parentId == Pockets 物品 id）；
//! - 添加物品：服务器 templates/handbook.json（分类树 + 物品归属）过滤出在物品库的
//!   条目，名称取 locale `{tpl} Name`、描述取 `{tpl} Description`（缺失空串）；
//! - 写回（先备份，op 列表逐条执行）：加钱（叠加现有货币堆，无则新建——表格口径
//!   简化，偏离参考的新建捆语义，注释留档）/ 附加行数 / 删除（含子孙物品递归清除，
//!   参考 FinalRemoveItems）/ 添加物品（简化放置引擎：仓库网格 CellsV 行 × CellsH 列
//!   + 附加行数，按现有物品 location 占位找空位，先横放后竖放 r=1，无空位则报错）。

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use tauri::AppHandle;

use crate::backup::backup_before_write;
use crate::character::database_dir;
use crate::profile::{is_safe_id, profiles_dir, SaveResult};

/// 三货币模板 id（参考 DefaultValues.Moneys*Tpl）
const RUBLE_TPL: &str = "5449016a4bdc2d6f028b456f";
const DOLLAR_TPL: &str = "5696686a4bdc2da3298b456a";
const EURO_TPL: &str = "569668774bdc2da2298b4568";

/// 装备槽固定序（稿图 09 装备子页槽序；slotId 为存档字段名；11-scav 复用）
pub(crate) const EQUIP_SLOTS: &[&str] = &[
    "Headwear",
    "Earmuffs",
    "FaceCover",
    "ArmBand",
    "ArmorVest",
    "Eyewear",
    "TacticalVest",
    "Backpack",
    "FirstPrimaryWeapon",
    "SecondPrimaryWeapon",
    "Holster",
    "Scabbard",
];

// ———————— 类型 ————————

/// 物品行（name 已本地化；count = StackObjectsCount 缺省 1）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StashItemRow {
    pub id: String,
    pub name: String,
    pub tpl: String,
    pub count: i64,
    /// tpl 不在服务器物品库（参考 IsInItemsDB = false，红色三角）
    pub modded: bool,
    pub is_weapon: bool,
    pub is_container: bool,
}

/// 装备槽行（item 为 None 即空槽）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EquipSlotRow {
    /// 存档 slotId 字段名（前端走 i18n 槽名映射）
    pub slot: String,
    pub item: Option<StashItemRow>,
}

/// 分类行（handbook Categories；name 已本地化）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddCategoryRow {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
}

/// 可添加物品行（desc 供「含描述」搜索；缺失空串）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddItemRow {
    pub tpl: String,
    pub name: String,
    pub desc: String,
    pub category_id: String,
}

/// 仓库页全量数据（仅 PMC）
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StashData {
    pub rubles: i64,
    pub dollars: i64,
    pub euros: i64,
    pub bonus_rows: i64,
    pub has_modded: bool,
    pub items: Vec<StashItemRow>,
    pub equipment: Vec<EquipSlotRow>,
    pub pockets: Vec<StashItemRow>,
    pub categories: Vec<AddCategoryRow>,
    pub addable: Vec<AddItemRow>,
}

/// 写回操作（op 列表逐条执行，一次备份）
#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum StashOp {
    AddMoney {
        tpl: String,
        amount: i64,
    },
    SetBonusRows {
        count: i64,
    },
    RemoveItems {
        ids: Vec<String>,
    },
    AddItem {
        tpl: String,
    },
    /// 把全局武器预设整树复制进仓库（根走放置引擎定位，零件挂根下原 slotId，
    /// 全部物品 id 重映射避免与档案冲突；图鉴「加入仓库」）
    AddBuild {
        preset_id: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StashSaveData {
    pub ops: Vec<StashOp>,
}

// ———————— 读侧 helpers ————————

/// 物品库：tpl → 条目引用索引（_props 关键字段 + _parent）
pub(crate) struct ItemsDb {
    /// tpl → (props, parent)
    entries: HashMap<String, ItemProps>,
    names: Option<Value>,
}

#[derive(Debug, Clone)]
struct ItemProps {
    width: i64,
    height: i64,
    stack_max: i64,
    grids: usize,
    recoil_force_up: f64,
}

impl ItemsDb {
    pub(crate) fn load(app: &AppHandle, names: Option<&Value>) -> ItemsDb {
        let mut entries = HashMap::new();
        if let Some(db) = database_dir(app) {
            if let Ok(txt) =
                fs::read_to_string(Path::new(&db).join("templates").join("items.json"))
            {
                // 真实 items.json 是 { tpl -> TarkovItem } 字典（AppData.cs LoadItemsDB 口径）
                if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&txt) {
                    for (tpl, it) in &map {
                        let p = it.get("_props");
                        let grids = p
                            .and_then(|x| x.get("Grids"))
                            .and_then(|g| g.as_array())
                            .map(|g| g.len())
                            .unwrap_or(0);
                        entries.insert(
                            tpl.clone(),
                            ItemProps {
                                width: p.and_then(|x| x.get("Width")).and_then(|v| v.as_f64()).unwrap_or(1.0) as i64,
                                height: p.and_then(|x| x.get("Height")).and_then(|v| v.as_f64()).unwrap_or(1.0) as i64,
                                stack_max: p
                                    .and_then(|x| x.get("StackMaxSize"))
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(1.0) as i64,
                                grids,
                                recoil_force_up: p
                                    .and_then(|x| x.get("RecoilForceUp"))
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(0.0),
                            },
                        );
                    }
                }
            }
        }
        ItemsDb {
            entries,
            names: names.cloned(),
        }
    }

    pub(crate) fn name(&self, tpl: &str) -> String {
        self.names
            .as_ref()
            .and_then(|n| n.get(format!("{} Name", tpl)))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| tpl.to_string())
    }

    /// tpl 是否武器（RecoilForceUp ≠ 0，与 item_row 口径一致；不在库 → false）
    pub(crate) fn is_weapon(&self, tpl: &str) -> bool {
        self.entries
            .get(tpl)
            .map(|p| p.recoil_force_up != 0.0)
            .unwrap_or(false)
    }
}

/// 新物品 id：sha1(时间戳 + 计数) 前 24 位 hex（参考 ObjectId 24 hex 形态）
pub(crate) fn new_item_id(seq: u32) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut hasher = Sha1::new();
    hasher.update(format!("{}:{}", nanos, seq).as_bytes());
    format!("{:x}", hasher.finalize())[..24].to_string()
}

/// 单物品行构造（name/modded/weapon/container 口径见模块头注释）
pub(crate) fn item_row(id: &str, tpl: &str, it: Option<&Value>, db: &ItemsDb) -> StashItemRow {
    let count = it
        .and_then(|x| x.pointer("/upd/StackObjectsCount"))
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0) as i64;
    let props = db.entries.get(tpl);
    let modded = props.is_none();
    StashItemRow {
        id: id.to_string(),
        name: db.name(tpl),
        tpl: tpl.to_string(),
        count,
        modded,
        is_weapon: props.map(|p| p.recoil_force_up != 0.0).unwrap_or(false),
        is_container: props.map(|p| p.grids > 0).unwrap_or(false),
    }
}

/// 档案语言（/info/language，缺失回退 en）对应的本地化表
pub(crate) fn locales_table(app: &AppHandle, lang: &str) -> Option<Value> {
    let db = database_dir(app)?;
    let txt = fs::read_to_string(
        Path::new(&db)
            .join("locales")
            .join("global")
            .join(format!("{}.json", lang)),
    )
    .or_else(|_| {
        fs::read_to_string(
            Path::new(&db)
                .join("locales")
                .join("global")
                .join("en.json"),
        )
    })
    .ok()?;
    serde_json::from_str::<Value>(&txt).ok()
}

/// 仓库页全量数据（仅 PMC）；档案缺失或不可读时为 None
pub fn load_stash(app: &AppHandle, id: &str) -> Option<StashData> {
    if !is_safe_id(id) {
        return None;
    }
    let dir = profiles_dir(app)?;
    let raw: Value =
        serde_json::from_str(&fs::read_to_string(Path::new(&dir).join(format!("{}.json", id))).ok()?)
            .ok()?;
    let pmc = raw.pointer("/characters/pmc")?;
    let inv = pmc.get("Inventory");
    let items = inv.and_then(|i| i.get("items")).and_then(|i| i.as_array())?;
    let stash_id = inv.and_then(|i| i.get("stash")).and_then(|v| v.as_str()).unwrap_or("");
    let equip_id = inv
        .and_then(|i| i.get("equipment"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let lang = raw
        .pointer("/info/language")
        .and_then(|v| v.as_str())
        .unwrap_or("en");
    let names = locales_table(app, lang);
    let db = ItemsDb::load(app, names.as_ref());

    // 口袋容器物品 id：parentId == equipment 且 slotId == "Pockets" 的条目
    let pockets_id = items
        .iter()
        .find(|it| {
            it.get("parentId").and_then(|v| v.as_str()) == Some(equip_id)
                && it.get("slotId").and_then(|v| v.as_str()) == Some("Pockets")
        })
        .and_then(|it| it.get("_id").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();

    let row_of = |it: &Value| -> Option<StashItemRow> {
        Some(item_row(
            it.get("_id")?.as_str()?,
            it.get("_tpl")?.as_str()?,
            Some(it),
            &db,
        ))
    };

    // 仓库表：parentId == stash 的直接子项
    let items_out: Vec<StashItemRow> = items
        .iter()
        .filter(|it| it.get("parentId").and_then(|v| v.as_str()) == Some(stash_id))
        .filter_map(row_of)
        .collect();

    // 货币聚合
    let money_sum = |tpl: &str| -> i64 {
        items
            .iter()
            .filter(|it| {
                it.get("_tpl").and_then(|v| v.as_str()) == Some(tpl)
                    && it.get("parentId").and_then(|v| v.as_str()) == Some(stash_id)
            })
            .filter_map(|it| it.pointer("/upd/StackObjectsCount").and_then(|v| v.as_f64()))
            .sum::<f64>() as i64
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

    // 口袋：parentId == Pockets 物品 id 的条目
    let pockets: Vec<StashItemRow> = items
        .iter()
        .filter(|it| it.get("parentId").and_then(|v| v.as_str()) == Some(pockets_id.as_str()))
        .filter_map(row_of)
        .collect();

    // 分类树 + 可添加物品：handbook.json（缺文件时空集，不阻塞页面）
    let (categories, addable) = load_addable(app, &db);

    Some(StashData {
        rubles: money_sum(RUBLE_TPL),
        dollars: money_sum(DOLLAR_TPL),
        euros: money_sum(EURO_TPL),
        bonus_rows: pmc.get("StashRowsBonusCount").and_then(|v| v.as_f64()).unwrap_or(0.0) as i64,
        has_modded: items_out.iter().any(|r| r.modded),
        items: items_out,
        equipment,
        pockets,
        categories,
        addable,
    })
}

/// 分类 + 可添加物品：handbook.json（Categories + Items），只收在物品库的条目
fn load_addable(app: &AppHandle, db: &ItemsDb) -> (Vec<AddCategoryRow>, Vec<AddItemRow>) {
    let Some(db_dir) = database_dir(app) else {
        return (Vec::new(), Vec::new());
    };
    let Ok(txt) = fs::read_to_string(
        Path::new(&db_dir).join("templates").join("handbook.json"),
    ) else {
        return (Vec::new(), Vec::new());
    };
    let Ok(v) = serde_json::from_str::<Value>(&txt) else {
        return (Vec::new(), Vec::new());
    };
    // 真实 handbook 分类只有 Id/ParentId/Icon，无 Name 字段；
    // 分类名 = locales 按「裸分类 Id」查（参考 HandbookCategory 本地化口径）
    let cat_name = |c: &Value| -> String {
        let key = c.get("Id").and_then(|n| n.as_str()).unwrap_or("");
        db.names
            .as_ref()
            .and_then(|n| n.get(key))
            .and_then(|x| x.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| key.to_string())
    };
    let categories: Vec<AddCategoryRow> = v
        .get("Categories")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| {
                    Some(AddCategoryRow {
                        id: c.get("Id")?.as_str()?.to_string(),
                        name: cat_name(c),
                        parent_id: c
                            .get("ParentId")
                            .and_then(|p| p.as_str())
                            .map(|s| s.to_string()),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let addable: Vec<AddItemRow> = v
        .get("Items")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|i| {
                    let tpl = i.get("Id")?.as_str()?;
                    // CanBeAddedToStash 口径：物品库存在且有本地化名（参考 TarkovItem）
                    if !db.entries.contains_key(tpl) {
                        return None;
                    }
                    Some(AddItemRow {
                        tpl: tpl.to_string(),
                        name: db.name(tpl),
                        desc: db
                            .names
                            .as_ref()
                            .and_then(|n| n.get(format!("{} Description", tpl)))
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string(),
                        category_id: i.get("ParentId")?.as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    (categories, addable)
}

// ———————— 写回 ————————

/// 收集物品及其全部子孙 id（参考 GetInnerItems 递归口径）
pub(crate) fn collect_subtree(ids: &[String], items: &[Value]) -> HashSet<String> {
    let mut out: HashSet<String> = ids.iter().cloned().collect();
    loop {
        let mut grew = false;
        for it in items {
            if let (Some(id), Some(pid)) = (
                it.get("_id").and_then(|v| v.as_str()),
                it.get("parentId").and_then(|v| v.as_str()),
            ) {
                if out.contains(pid) && !out.contains(id) {
                    out.insert(id.to_string());
                    grew = true;
                }
            }
        }
        if !grew {
            break;
        }
    }
    out
}

/// 仓库页写回：先强制备份（领域铁律），后逐条执行 op 列表
pub fn save_stash(app: &AppHandle, id: &str, data: &StashSaveData) -> Result<SaveResult, String> {
    if !is_safe_id(id) {
        return Err(format!("Invalid profile id: {}", id));
    }
    let dir = profiles_dir(app).ok_or_else(|| "未选择 SPT 服务器".to_string())?;
    let path = Path::new(&dir).join(format!("{}.json", id));
    let txt = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut raw: Value = serde_json::from_str(&txt).map_err(|e| e.to_string())?;
    let backup_path = backup_before_write(app, &path)?;

    let lang = raw
        .pointer("/info/language")
        .and_then(|v| v.as_str())
        .unwrap_or("en")
        .to_string();
    let names = locales_table(app, &lang);
    let db = ItemsDb::load(app, names.as_ref());
    let stash_id = raw
        .pointer("/characters/pmc/Inventory/stash")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    for (seq, op) in data.ops.iter().enumerate() {
        match op {
            StashOp::AddMoney { tpl, amount } => {
                let amount = *amount;
                // 表格口径简化：优先叠加现有货币堆（不占新格子），无堆则新建一捆
                let items = raw
                    .pointer_mut("/characters/pmc/Inventory/items")
                    .and_then(|v| v.as_array_mut())
                    .ok_or("Inventory.items 缺失")?;
                let stack = items
                    .iter_mut()
                    .find(|it| {
                        it.get("_tpl").and_then(|v| v.as_str()) == Some(tpl.as_str())
                            && it.get("parentId").and_then(|v| v.as_str()) == Some(stash_id.as_str())
                    })
                    .and_then(|it| it.pointer_mut("/upd/StackObjectsCount"));
                if let Some(cur) = stack {
                    let cap = db.entries.get(tpl).map(|p| p.stack_max).unwrap_or(i64::MAX);
                    let v = (cur.as_f64().unwrap_or(0.0) as i64 + amount.max(0)).min(cap);
                    *cur = json!(v);
                } else {
                    let cap = db.entries.get(tpl).map(|p| p.stack_max).unwrap_or(i64::MAX);
                    items.push(json!({
                        "_id": new_item_id(seq as u32),
                        "_tpl": tpl,
                        "parentId": stash_id,
                        "slotId": "hideout",
                        "upd": { "StackObjectsCount": amount.max(0).min(cap) }
                    }));
                }
            }
            StashOp::SetBonusRows { count } => {
                let count = *count;
                if let Some(pmc) = raw.pointer_mut("/characters/pmc") {
                    pmc["StashRowsBonusCount"] = json!(count.max(0));
                }
            }
            StashOp::RemoveItems { ids } => {
                let items = raw
                    .pointer_mut("/characters/pmc/Inventory/items")
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
            StashOp::AddItem { tpl } => {
                let db_dir = database_dir(app).ok_or("未选择 SPT 服务器")?;
                add_item_to_stash(&mut raw, &stash_id, tpl, &db_dir, &db, seq as u32)?;
            }
            StashOp::AddBuild { preset_id } => {
                let db_dir = database_dir(app).ok_or("未选择 SPT 服务器")?;
                add_build_to_stash(&mut raw, &stash_id, preset_id, &db_dir, &db, seq as u32)?;
            }
        }
    }

    let out = serde_json::to_string_pretty(&raw).map_err(|e| e.to_string())?;
    fs::write(&path, out).map_err(|e| e.to_string())?;
    Ok(SaveResult {
        backup_path: backup_path.map(|p| p.to_string_lossy().to_string()),
    })
}

/// 添加物品（简化放置引擎，模块头注释有口径说明）
fn add_item_to_stash(
    raw: &mut Value,
    stash_id: &str,
    tpl: &str,
    db_dir: &str,
    db: &ItemsDb,
    seq: u32,
) -> Result<(), String> {
    let props = db
        .entries
        .get(tpl)
        .ok_or_else(|| format!("未知物品模板: {}", tpl))?;
    let stack_max = props.stack_max.max(1);
    let (place_x, place_y, rotated) = find_free_slot(raw, stash_id, tpl, db_dir, db)?;
    let items = raw
        .pointer_mut("/characters/pmc/Inventory/items")
        .and_then(|v| v.as_array_mut())
        .ok_or("Inventory.items 缺失")?;
    let mut new_it = json!({
        "_id": new_item_id(seq),
        "_tpl": tpl,
        "parentId": stash_id,
        "slotId": "hideout",
        "location": { "x": place_x, "y": place_y, "r": if rotated { 1 } else { 0 } }
    });
    if stack_max > 1 {
        new_it["upd"] = json!({ "StackObjectsCount": 1 });
    }
    items.push(new_it);
    Ok(())
}

/// 放置引擎（模块头注释有口径说明）：仓库网格 = stash 模板 Grids[0] 的 CellsV 行 ×
/// CellsH 列 + 附加行数；已有物品按 location 占位（r=1 横竖互换）；行优先扫描找空位，
/// 先横放 (W×H)，放不下再竖放 (H×W, r=1)；无空位报错（不静默丢弃）
fn find_free_slot(
    raw: &Value,
    stash_id: &str,
    tpl: &str,
    db_dir: &str,
    db: &ItemsDb,
) -> Result<(i64, i64, bool), String> {
    let props = db
        .entries
        .get(tpl)
        .ok_or_else(|| format!("未知物品模板: {}", tpl))?;
    let (w, h) = (props.width.max(1), props.height.max(1));

    let items_arr = raw
        .pointer("/characters/pmc/Inventory/items")
        .and_then(|v| v.as_array())
        .ok_or("Inventory.items 缺失")?;
    let stash_item = items_arr
        .iter()
        .find(|it| it.get("_id").and_then(|v| v.as_str()) == Some(stash_id))
        .ok_or("仓库容器缺失")?;
    let stash_tpl = stash_item
        .get("_tpl")
        .and_then(|v| v.as_str())
        .ok_or("仓库容器缺模板")?;
    let (rows, cols) = stash_grid_size(db_dir, stash_tpl)?;
    let bonus = raw
        .pointer("/characters/pmc/StashRowsBonusCount")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as i64;

    let rows_total = (rows + bonus).max(h);
    let cols_total = cols.max(w);
    // 占位网格
    let mut occ = vec![vec![false; cols_total as usize]; rows_total as usize];
    for it in items_arr.iter() {
        if it.get("parentId").and_then(|v| v.as_str()) != Some(stash_id) {
            continue;
        }
        let (Some(loc), Some(itpl)) = (
            it.get("location"),
            it.get("_tpl").and_then(|v| v.as_str()),
        ) else {
            continue;
        };
        let (Some(x), Some(y)) = (
            loc.get("x").and_then(|v| v.as_f64()),
            loc.get("y").and_then(|v| v.as_f64()),
        ) else {
            continue;
        };
        let r = loc.get("r").and_then(|v| v.as_f64()).unwrap_or(0.0) as i64;
        let (iw, ih) = db
            .entries
            .get(itpl)
            .map(|p| {
                if r == 1 {
                    (p.height, p.width)
                } else {
                    (p.width, p.height)
                }
            })
            .unwrap_or((1, 1));
        for yy in y as usize..((y as i64 + ih).min(rows_total) as usize) {
            for xx in x as usize..((x as i64 + iw).min(cols_total) as usize) {
                occ[yy][xx] = true;
            }
        }
    }

    // 行优先扫描找空位：先横放 (w×h)，再竖放 (h×w, r=1)
    let fits = |oy: i64, ox: i64, iw: i64, ih: i64| -> bool {
        oy >= 0
            && ox >= 0
            && oy + ih <= rows_total
            && ox + iw <= cols_total
            && ((oy as usize)..((oy + ih) as usize))
                .all(|yy| ((ox as usize)..((ox + iw) as usize)).all(|xx| !occ[yy][xx]))
    };
    (0..rows_total - h + 1)
        .flat_map(|y| (0..cols_total - w + 1).map(move |x| (y, x)))
        .find_map(|(y, x)| fits(y, x, w, h).then_some((x, y, false)))
        .or_else(|| {
            (0..rows_total - w + 1)
                .flat_map(|y| (0..cols_total - h + 1).map(move |x| (y, x)))
                .find_map(|(y, x)| fits(y, x, h, w).then_some((x, y, true)))
        })
        .ok_or("仓库已满，没有空位放置该物品".to_string())
}

/// 把预设物品树整树复制进仓库：根走放置引擎，零件挂根下原 slotId（无 location），
/// 全部物品 id 重映射避免与档案冲突（10 屏 / 12 屏「加入仓库」共用）
pub(crate) fn place_build_tree(
    raw: &mut Value,
    stash_id: &str,
    root_id: &str,
    pitems: &[Value],
    db_dir: &str,
    db: &ItemsDb,
    seq: u32,
) -> Result<(), String> {
    let root_tpl = pitems
        .iter()
        .find(|it| it.get("_id").and_then(|x| x.as_str()) == Some(root_id))
        .and_then(|it| it.get("_tpl").and_then(|x| x.as_str()))
        .ok_or("预设缺根模板")?
        .to_string();

    let (place_x, place_y, rotated) = find_free_slot(raw, stash_id, &root_tpl, db_dir, db)?;

    // id 重映射
    let mut id_map: HashMap<String, String> = HashMap::new();
    let mut nseq = seq;
    for it in pitems {
        if let Some(old) = it.get("_id").and_then(|x| x.as_str()) {
            nseq = nseq.wrapping_add(1);
            id_map.insert(old.to_string(), new_item_id(nseq));
        }
    }

    let items = raw
        .pointer_mut("/characters/pmc/Inventory/items")
        .and_then(|v| v.as_array_mut())
        .ok_or("Inventory.items 缺失")?;
    for it in pitems {
        let Some(old_id) = it.get("_id").and_then(|x| x.as_str()) else {
            continue;
        };
        let Some(new_id) = id_map.get(old_id) else {
            continue;
        };
        let tpl = it.get("_tpl").and_then(|x| x.as_str()).unwrap_or("");
        let is_root = old_id == root_id;
        let new_parent = it
            .get("parentId")
            .and_then(|x| x.as_str())
            .and_then(|p| id_map.get(p).cloned())
            .unwrap_or_else(|| stash_id.to_string());
        let mut obj = json!({
            "_id": new_id,
            "_tpl": tpl,
            "parentId": new_parent,
            "slotId": it.get("slotId").and_then(|x| x.as_str()).unwrap_or(if is_root { "hideout" } else { "" })
        });
        if is_root {
            obj["slotId"] = json!("hideout");
            obj["location"] = json!({ "x": place_x, "y": place_y, "r": if rotated { 1 } else { 0 } });
        }
        items.push(obj);
    }
    Ok(())
}

/// 把全局武器预设整树复制进仓库（薄壳：读 globals 取根与物品表后调 place_build_tree）
fn add_build_to_stash(
    raw: &mut Value,
    stash_id: &str,
    preset_id: &str,
    db_dir: &str,
    db: &ItemsDb,
    seq: u32,
) -> Result<(), String> {
    let txt = fs::read_to_string(Path::new(db_dir).join("globals.json"))
        .map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&txt).map_err(|e| e.to_string())?;
    let preset = v
        .pointer("/config/ItemPresets")
        .and_then(|p| p.get(preset_id))
        .ok_or("查无该预设")?;
    let Some(root_id) = preset.get("root").and_then(|r| r.as_str()) else {
        return Err("预设缺根物品".to_string());
    };
    let pitems = preset
        .get("items")
        .and_then(|i| i.as_array())
        .ok_or("预设缺物品表")?;
    place_build_tree(raw, stash_id, root_id, pitems, db_dir, db, seq)
}

/// 仓库网格尺寸：stash 模板 Grids[0]._props 的 CellsV（行）/ CellsH（列），
/// 从 items.json 原始条目回读（ItemsDb 只存了计数，不存格径）
fn stash_grid_size(db_dir: &str, stash_tpl: &str) -> Result<(i64, i64), String> {
    let txt = fs::read_to_string(
        Path::new(db_dir).join("templates").join("items.json"),
    )
    .map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&txt).map_err(|e| e.to_string())?;
    // 真实 items.json 是字典：{ tpl -> TarkovItem }
    let stash = v
        .get(stash_tpl)
        .ok_or("仓库模板不在物品库")?;
    let g = stash
        .pointer("/_props/Grids/0/_props")
        .ok_or("仓库模板无网格")?;
    Ok((
        g.get("CellsV").and_then(|x| x.as_f64()).unwrap_or(0.0) as i64,
        g.get("CellsH").and_then(|x| x.as_f64()).unwrap_or(0.0) as i64,
    ))
}
