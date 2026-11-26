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

/// 装备槽固定序（稿图 09 装备子页槽序；slotId 为存档字段名；11-scav 复用）。
/// 槽名对照 4.1.1 参考：Earpiece（DefaultValues.EarpieceSlotId，旧名 Earmuffs 系笔误）、
/// SecuredContainer（EquipmentAdapter 有此槽，漏了会导致安全容器不可见 / scav 清空装备漏删其内物品）
pub(crate) const EQUIP_SLOTS: &[&str] = &[
    "Headwear",
    "Earpiece",
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
    "SecuredContainer",
];

// ———————— 类型 ————————

/// 按模板 id 汇总整份库存里的货币总额。
/// 参考 4.1.1 `CharacterInventory.GetMoneyCountString`：
/// `Items.Where(x => x.Tpl == moneys).Sum(x => x.Upd.StackObjectsCount ?? 0)` —— 不限父级，
/// 因此钱装在钱箱 / SICC 包 / 口袋里都能算到（旧实现只认 stash 直接子项，装了钱箱就显示 0）。
pub(crate) fn sum_money(items: &[Value], tpl: &str) -> i64 {
    items
        .iter()
        .filter(|it| it.get("_tpl").and_then(|v| v.as_str()) == Some(tpl))
        .filter_map(|it| {
            it.pointer("/upd/StackObjectsCount")
                .and_then(|v| v.as_f64())
        })
        .sum::<f64>() as i64
}

/// 仓库附加行数：真值在角色 `Bonuses[]`（type="StashRows"）的 value。
/// 参考 4.1.1 CharacterBonus.cs / Character.cs——`StashRowsBonusCount` 只是计算属性，
/// 存档 JSON 里没有这个键（此前读幻键恒 0，已修）
fn stash_rows_bonus(pmc: &Value) -> i64 {
    pmc.get("Bonuses")
        .and_then(|v| v.as_array())
        .and_then(|arr| {
            arr.iter()
                .find(|b| b.get("type").and_then(|v| v.as_str()) == Some("StashRows"))
        })
        .and_then(|b| b.get("value"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as i64
}

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
    /// 模板占格（items.json `_props.Width/Height`，缺失按 1×1）
    pub w: i64,
    pub h: i64,
    /// 格位坐标；容器内子件没有 location，统一 -1（前端据此不画格子）
    pub x: i64,
    pub y: i64,
    /// location.r == "Vertical"（旋转 90°）
    pub rot: bool,
    /// 口袋物品专用的格名（pocket1..pocket4）；其他行不序列化
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slot_id: Option<String>,
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

/// 可添加物品行（desc 供「含描述」搜索；缺失空串；shortName 供按 M4A1 这类短名搜索）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddItemRow {
    pub tpl: String,
    pub name: String,
    pub short_name: String,
    pub desc: String,
    pub category_id: String,
    /// 模板占格（1×1 才放得进口袋）
    pub w: i64,
    pub h: i64,
}

/// 仓库页全量数据（仅 PMC）
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StashData {
    pub rubles: i64,
    pub dollars: i64,
    pub euros: i64,
    pub bonus_rows: i64,
    /// 仓库网格：列数（cellsH）/ 行数（cellsV），取不到时为 0
    pub grid_cols: i64,
    pub grid_rows: i64,
    pub has_modded: bool,
    pub items: Vec<StashItemRow>,
    pub equipment: Vec<EquipSlotRow>,
    pub pockets: Vec<StashItemRow>,
    pub categories: Vec<AddCategoryRow>,
    pub addable: Vec<AddItemRow>,
}

/// 写回操作（op 列表逐条执行，一次备份）
#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
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
        /// 要加的数量；旧前端只传 tpl 时按 1 处理
        #[serde(default)]
        count: Option<i64>,
    },
    /// 把全局武器预设整树复制进仓库（根走放置引擎定位，零件挂根下原 slotId，
    /// 全部物品 id 重映射避免与档案冲突；图鉴「加入仓库」）
    AddBuild {
        preset_id: String,
    },
    /// 仓库顶层物品拖动到指定格位。只改 location.x / y / r，
    /// 其他字段一字不动；容器内（钱箱、背包里）的物品不允许直接移动。
    MoveItem {
        id: String,
        x: i64,
        y: i64,
        #[serde(default)]
        rot: bool,
    },
    /// 往口袋（Pockets 容器）放一件物品：占用第一个空口袋格。
    /// 口袋格是 1×1，只有 1×1 的物品放得进去；四个口袋都满时报错，不静默丢弃。
    AddToPocket {
        tpl: String,
    },
    /// 口袋内换位：把 `id` 挪到 `slot`（pocket1..pocket4）；目标格有物品时两者互换。
    MoveInPocket {
        id: String,
        slot: String,
    },
    /// 口袋取出到仓库：根走仓库放置引擎，取出后留在仓库顶层。
    PocketTakeOut {
        id: String,
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
    pub(crate) entries: HashMap<String, ItemProps>,
    names: Option<Value>,
}

#[derive(Debug, Clone)]
pub(crate) struct ItemProps {
    width: i64,
    height: i64,
    pub(crate) stack_max: i64,
    grids: usize,
    recoil_force_up: f64,
    /// `_props.ShortName`（如 M4A1），供搜索按短名命中
    short_name: String,
}

impl ItemsDb {
    pub(crate) fn load(app: &AppHandle, names: Option<&Value>) -> ItemsDb {
        match database_dir(app) {
            Some(db) => Self::load_from(Path::new(&db), names),
            None => ItemsDb {
                entries: HashMap::new(),
                names: names.cloned(),
            },
        }
    }

    /// 直接吃 database 目录，便于不依赖 AppHandle 的真机自检
    pub(crate) fn load_from(db: &Path, names: Option<&Value>) -> ItemsDb {
        let mut entries = HashMap::new();
        if let Ok(txt) = fs::read_to_string(db.join("templates").join("items.json")) {
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
                            short_name: p
                                .and_then(|x| x.get("ShortName"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                        },
                    );
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

    /// `_props.ShortName`（如 M4A1）；缺失空串
    pub(crate) fn short_name(&self, tpl: &str) -> String {
        self.entries
            .get(tpl)
            .map(|p| p.short_name.clone())
            .unwrap_or_default()
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
    // 格位：只有仓库/容器的直接子项带 location；装备槽与内嵌零件没有
    let loc = it.and_then(|x| x.get("location"));
    let coord = |k: &str| -> i64 {
        loc.and_then(|l| l.get(k))
            .and_then(|v| v.as_f64())
            .map(|v| v as i64)
            .unwrap_or(-1)
    };
    let rot = loc
        .and_then(|l| l.get("r"))
        .and_then(|v| v.as_str())
        .map(|s| s.eq_ignore_ascii_case("Vertical"))
        .unwrap_or(false);
    StashItemRow {
        id: id.to_string(),
        name: db.name(tpl),
        tpl: tpl.to_string(),
        count,
        modded,
        is_weapon: props.map(|p| p.recoil_force_up != 0.0).unwrap_or(false),
        is_container: props.map(|p| p.grids > 0).unwrap_or(false),
        w: props.map(|p| p.width).unwrap_or(1).max(1),
        h: props.map(|p| p.height).unwrap_or(1).max(1),
        x: coord("x"),
        y: coord("y"),
        rot,
        slot_id: None,
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

    let lang = crate::server::data_language(app, Some(&raw));
    let names = locales_table(app, &lang);
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

    // 货币聚合：按 tpl 全量求和，不限父级 ——
    // 参考 CharacterInventory.GetMoneyCountString 就是 `Items.Where(x => x.Tpl == moneys).Sum(...)`；
    // 玩家常常把钱塞在钱箱/SICC 包里（本机实测两个 59fb016586f7746d0d4b423a 钱箱内各躺 3700 万），
    // 只统计 stash 直接子项会显示 0。
    let money_sum = |tpl: &str| -> i64 { sum_money(items, tpl) };

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
    let mut pockets: Vec<StashItemRow> = items
        .iter()
        .filter(|it| it.get("parentId").and_then(|v| v.as_str()) == Some(pockets_id.as_str()))
        .filter_map(row_of)
        .collect();
    // 口袋物品带格名（pocket1..pocket4），前端按它画四个口袋格并支持换格
    for row in pockets.iter_mut() {
        row.slot_id = items
            .iter()
            .find(|it| it.get("_id").and_then(|v| v.as_str()) == Some(row.id.as_str()))
            .and_then(|it| it.get("slotId"))
            .and_then(|v| v.as_str())
            .map(str::to_string);
    }

    // 分类树 + 可添加物品：handbook.json（缺文件时空集，不阻塞页面）
    let (categories, addable) = load_addable(app, &db);

    // 仓库网格：stash 容器的模板格径（cellsV 行 / cellsH 列）；取不到时留 0，前端退回列表视图
    let stash_tpl = items
        .iter()
        .find(|it| it.get("_id").and_then(|v| v.as_str()) == Some(stash_id))
        .and_then(|it| it.get("_tpl"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let (grid_rows, grid_cols) = match database_dir(app) {
        Some(dir) => stash_grid_size(&dir, stash_tpl).unwrap_or((0, 0)),
        None => (0, 0),
    };

    Some(StashData {
        rubles: money_sum(RUBLE_TPL),
        dollars: money_sum(DOLLAR_TPL),
        euros: money_sum(EURO_TPL),
        bonus_rows: stash_rows_bonus(pmc),
        grid_cols,
        grid_rows,
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
                        short_name: db.short_name(tpl),
                        desc: db
                            .names
                            .as_ref()
                            .and_then(|n| n.get(format!("{} Description", tpl)))
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string(),
                        category_id: i.get("ParentId")?.as_str()?.to_string(),
                        w: db.entries.get(tpl).map(|p| p.width.max(1)).unwrap_or(1),
                        h: db.entries.get(tpl).map(|p| p.height.max(1)).unwrap_or(1),
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

    let lang = crate::server::data_language(app, Some(&raw));
    let names = locales_table(app, &lang);
    let db = ItemsDb::load(app, names.as_ref());
    let stash_id = raw
        .pointer("/characters/pmc/Inventory/stash")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // 口袋容器 id（equipment 下 slotId == Pockets 的那件）；没有口袋的档案按空处理
    let pockets_id = pockets_of(&raw).map(|(id, _)| id).unwrap_or_default();

    for (seq, op) in data.ops.iter().enumerate() {
        match op {
            StashOp::AddMoney { tpl, amount } => {
                let amount = *amount;
                let cap = db.entries.get(tpl).map(|p| p.stack_max).unwrap_or(i64::MAX);
                // 叠加口径与读取一致：不限父级（钱多在钱箱 / SICC 包里），
                // 先找一捆还有余量的同类货币叠加，全部满了才在仓库顶层开新捆。
                let mut left = amount.max(0);
                {
                    let slots = raw
                        .pointer_mut("/characters/pmc/Inventory/items")
                        .and_then(Value::as_array_mut)
                        .ok_or("Inventory.items 缺失")?;
                    for it in slots.iter_mut() {
                        if left <= 0 {
                            break;
                        }
                        if it.get("_tpl").and_then(Value::as_str) != Some(tpl.as_str()) {
                            continue;
                        }
                        let Some(cur) = it.pointer_mut("/upd/StackObjectsCount") else {
                            continue;
                        };
                        let now = cur.as_f64().unwrap_or(0.0) as i64;
                        let room = (cap - now).max(0);
                        if room == 0 {
                            continue;
                        }
                        let add = room.min(left);
                        *cur = json!(now + add);
                        left -= add;
                    }
                }
                if left > 0 {
                    let count = left.min(cap);
                    let db_dir = database_dir(app).ok_or("未选择 SPT 服务器")?;
                    let (place_x, place_y, rotated) =
                        find_free_slot(&raw, &stash_id, tpl, &db_dir, &db)?;
                    let items = raw
                        .pointer_mut("/characters/pmc/Inventory/items")
                        .and_then(Value::as_array_mut)
                        .ok_or("Inventory.items 缺失")?;
                    items.push(json!({
                        "_id": new_item_id(seq as u32),
                        "_tpl": tpl,
                        "parentId": stash_id,
                        "slotId": "hideout",
                        "location": { "x": place_x, "y": place_y, "r": if rotated { "Vertical" } else { "Horizontal" } },
                        "upd": { "StackObjectsCount": count }
                    }));
                }
            }
            StashOp::SetBonusRows { count } => {
                // 真值在 Bonuses[]（type="StashRows"）：0 → 删项、奇数 → 进位偶数、无项 → 建项
                //（对照 4.1.1 Character.cs setter + ProfileSaver.WriteStashBonus；
                //  此前写幻键 StashRowsBonusCount，游戏不认）
                let count = {
                    let c = (*count).max(0);
                    if c % 2 > 0 { c + 1 } else { c }
                };
                let Some(pmc) = raw.pointer_mut("/characters/pmc") else {
                    return Err("characters/pmc 缺失".into());
                };
                if !pmc.get("Bonuses").map(Value::is_array).unwrap_or(false) {
                    if count == 0 {
                        continue;
                    }
                    pmc["Bonuses"] = json!([]);
                }
                let Some(bonuses) = pmc.get_mut("Bonuses").and_then(Value::as_array_mut) else {
                    return Err("Bonuses 非数组".into());
                };
                match bonuses
                    .iter()
                    .position(|b| b.get("type").and_then(Value::as_str) == Some("StashRows"))
                {
                    Some(i) if count == 0 => {
                        bonuses.remove(i);
                    }
                    Some(i) => {
                        bonuses[i]["value"] = json!(count);
                    }
                    None if count > 0 => {
                        bonuses.push(json!({
                            "id": "fac7b58e9883cd4945a8d5b0",
                            "value": count,
                            "type": "StashRows",
                            "passive": true,
                            "visible": true,
                            "production": false
                        }));
                    }
                    None => {}
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
            StashOp::AddItem { tpl, count } => {
                let db_dir = database_dir(app).ok_or("未选择 SPT 服务器")?;
                add_item_to_stash(
                    &mut raw,
                    &stash_id,
                    tpl,
                    &db_dir,
                    &db,
                    seq as u32,
                    count.unwrap_or(1),
                )?;
            }
            StashOp::AddBuild { preset_id } => {
                let db_dir = database_dir(app).ok_or("未选择 SPT 服务器")?;
                add_build_to_stash(&mut raw, &stash_id, preset_id, &db_dir, &db, seq as u32)?;
            }
            StashOp::MoveItem { id, x, y, rot } => {
                let db_dir = database_dir(app).ok_or("未选择 SPT 服务器")?;
                move_item_in_stash(&mut raw, &stash_id, id, *x, *y, *rot, &db_dir, &db)?;
            }
            StashOp::AddToPocket { tpl } => {
                if pockets_id.is_empty() {
                    return Err("当前档案没有口袋".into());
                }
                let db_dir = database_dir(app).ok_or("未选择 SPT 服务器")?;
                add_item_to_pockets(&mut raw, &pockets_id, tpl, &db_dir, &db, seq as u32)?;
            }
            StashOp::MoveInPocket { id, slot } => {
                if pockets_id.is_empty() {
                    return Err("当前档案没有口袋".into());
                }
                let db_dir = database_dir(app).ok_or("未选择 SPT 服务器")?;
                move_in_pocket(&mut raw, &pockets_id, id, slot, &db_dir)?;
            }
            StashOp::PocketTakeOut { id } => {
                if pockets_id.is_empty() {
                    return Err("当前档案没有口袋".into());
                }
                let db_dir = database_dir(app).ok_or("未选择 SPT 服务器")?;
                pocket_take_out(&mut raw, &stash_id, &pockets_id, id, &db_dir, &db)?;
            }
        }
    }

    let out = serde_json::to_string(&raw).map_err(|e| e.to_string())?;
    fs::write(&path, out).map_err(|e| e.to_string())?;
    Ok(SaveResult {
        backup_path: backup_path.map(|p| p.to_string_lossy().to_string()),
    })
}

/// 添加物品（简化放置引擎，模块头注释有口径说明）
///
/// `want` 为要加的数量：可堆叠物品先补满仓库里已有的同类堆，再开新叠；
/// 不可堆叠物品一件一个。上限 100，避免一次点击把仓库塞爆。
pub(crate) fn add_item_to_stash(
    raw: &mut Value,
    stash_id: &str,
    tpl: &str,
    db_dir: &str,
    db: &ItemsDb,
    seq: u32,
    want: i64,
) -> Result<(), String> {
    let props = db
        .entries
        .get(tpl)
        .ok_or_else(|| format!("未知物品模板: {}", tpl))?;
    let stack_max = props.stack_max.max(1);
    let mut left = want.clamp(1, 100);

    // 先把仓库里已有的同类堆补满（只有直接挂在仓库下的算，容器内的不动 —— 与加钱口径一致）
    if stack_max > 1 {
        let ids: Vec<String> = raw
            .pointer("/characters/pmc/Inventory/items")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter(|it| {
                        it.get("parentId").and_then(Value::as_str) == Some(stash_id)
                            && it.get("_tpl").and_then(Value::as_str) == Some(tpl)
                    })
                    .filter_map(|it| it.get("_id").and_then(Value::as_str).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        for id in ids {
            if left <= 0 {
                break;
            }
            let Some(items) = raw
                .pointer_mut("/characters/pmc/Inventory/items")
                .and_then(Value::as_array_mut)
            else {
                break;
            };
            let Some(it) = items
                .iter_mut()
                .find(|i| i.get("_id").and_then(Value::as_str) == Some(id.as_str()))
            else {
                continue;
            };
            let Some(cur) = it.pointer_mut("/upd/StackObjectsCount") else {
                continue;
            };
            let now = cur.as_f64().unwrap_or(0.0) as i64;
            let room = (stack_max - now).max(0);
            if room == 0 {
                continue;
            }
            let add = room.min(left);
            *cur = json!(now + add);
            left -= add;
        }
    }

    // 剩余的开新叠/新件
    while left > 0 {
        let count = left.min(stack_max);
        let (place_x, place_y, rotated) = find_free_slot(raw, stash_id, tpl, db_dir, db)?;
        let items = raw
            .pointer_mut("/characters/pmc/Inventory/items")
            .and_then(|v| v.as_array_mut())
            .ok_or("Inventory.items 缺失")?;
        let mut new_it = json!({
            "_id": new_item_id(seq.wrapping_add(items.len() as u32)),
            "_tpl": tpl,
            "parentId": stash_id,
            "slotId": "hideout",
            "location": { "x": place_x, "y": place_y, "r": if rotated { "Vertical" } else { "Horizontal" } }
        });
        if stack_max > 1 {
            new_it["upd"] = json!({ "StackObjectsCount": count });
        }
        items.push(new_it);
        left -= count;
    }
    Ok(())
}

/// 放置引擎（模块头注释有口径说明）：仓库网格 = stash 模板 Grids[0] 的 CellsV 行 ×
/// CellsH 列 + 附加行数；已有物品按 location 占位（r=1 横竖互换）；行优先扫描找空位，
/// 先横放 (W×H)，放不下再竖放 (H×W, r=1)；无空位报错（不静默丢弃）
/// 把仓库顶层某件物品挪到 (x, y)，rot 为 true 时宽高互换。
/// 只改 `location.x/y/r`，其余字段（isSearched 等）原样保留；
/// 越界、撞其它物品、或物品不在仓库顶层（在容器里）都直接报错，不做半截写入。
pub(crate) fn move_item_in_stash(
    raw: &mut Value,
    stash_id: &str,
    id: &str,
    x: i64,
    y: i64,
    rot: bool,
    db_dir: &str,
    db: &ItemsDb,
) -> Result<(), String> {
    if x < 0 || y < 0 {
        return Err("落点超出仓库范围".into());
    }
    // 第一步只读：确认目标在仓库顶层，并收集其他顶层物品的占位矩形
    let (stash_tpl, w, h, others) = {
        let items = raw
            .pointer("/characters/pmc/Inventory/items")
            .and_then(|v| v.as_array())
            .ok_or("Inventory.items 缺失")?;
        let stash_tpl = items
            .iter()
            .find(|it| it.get("_id").and_then(|v| v.as_str()) == Some(stash_id))
            .and_then(|it| it.get("_tpl"))
            .and_then(|v| v.as_str())
            .ok_or("仓库容器缺失")?
            .to_string();
        let target = items
            .iter()
            .find(|it| it.get("_id").and_then(|v| v.as_str()) == Some(id))
            .ok_or("找不到要移动的物品")?;
        if target.get("parentId").and_then(|v| v.as_str()) != Some(stash_id) {
            return Err("只能移动仓库里直接摆放的物品，容器内的东西请先在游戏里取出".into());
        }
        let tpl = target
            .get("_tpl")
            .and_then(|v| v.as_str())
            .ok_or("物品缺模板")?;
        let props = db
            .entries
            .get(tpl)
            .ok_or_else(|| format!("未知物品模板: {}", tpl))?;
        let (bw, bh) = (props.width.max(1), props.height.max(1));
        let mut occ: Vec<(i64, i64, i64, i64)> = Vec::new();
        for it in items.iter() {
            if it.get("_id").and_then(|v| v.as_str()) == Some(id) {
                continue;
            }
            if it.get("parentId").and_then(|v| v.as_str()) != Some(stash_id) {
                continue;
            }
            let (Some(loc), Some(t)) = (
                it.get("location"),
                it.get("_tpl").and_then(|v| v.as_str()),
            ) else {
                continue;
            };
            let (Some(ox), Some(oy)) = (
                loc.get("x").and_then(|v| v.as_i64()),
                loc.get("y").and_then(|v| v.as_i64()),
            ) else {
                continue;
            };
            let (ow, oh) = db
                .entries
                .get(t)
                .map(|p| (p.width.max(1), p.height.max(1)))
                .unwrap_or((1, 1));
            let r = is_vertical(loc);
            occ.push(if r { (ox, oy, oh, ow) } else { (ox, oy, ow, oh) });
        }
        (stash_tpl, bw, bh, occ)
    };

    // 第二步：网格尺寸（读不到就拒绝，不猜）
    let (rows, cols) = {
        let (r, c) = stash_grid_size(db_dir, &stash_tpl)?;
        let bonus = raw
            .pointer("/characters/pmc")
            .map(stash_rows_bonus)
            .unwrap_or(0);
        (r + bonus, c)
    };

    let (w, h) = if rot { (h, w) } else { (w, h) };
    if x + w > cols || y + h > rows {
        return Err("落点超出仓库范围".into());
    }
    if others
        .iter()
        .any(|&(ox, oy, ow, oh)| x < ox + ow && ox < x + w && y < oy + oh && oy < y + h)
    {
        return Err("这里已经有东西了".into());
    }

    let items = raw
        .pointer_mut("/characters/pmc/Inventory/items")
        .and_then(|v| v.as_array_mut())
        .ok_or("Inventory.items 缺失")?;
    let target = items
        .iter_mut()
        .find(|it| it.get("_id").and_then(|v| v.as_str()) == Some(id))
        .ok_or("找不到要移动的物品")?;
    let obj = target.as_object_mut().ok_or("物品结构异常")?;
    let loc = obj
        .entry("location")
        .or_insert_with(|| json!({ "x": 0, "y": 0, "r": "Horizontal" }));
    if let Some(l) = loc.as_object_mut() {
        // 只写真正变化的值：真实档里很多物品的 location 只有 x/y（没有 r），
        // 无条件 insert 会凭空多出 r，存档就被改脏了
        if l.get("x").and_then(|v| v.as_i64()) != Some(x) {
            l.insert("x".into(), json!(x));
        }
        if l.get("y").and_then(|v| v.as_i64()) != Some(y) {
            l.insert("y".into(), json!(y));
        }
    }
    if is_vertical(loc) != rot {
        set_vertical(loc, rot);
    }
    Ok(())
}

/// 物品是否竖放：真实存档里 `location.r` 是字符串 `"Vertical"` / `"Horizontal"`，
/// 旧代码按布尔读写，导致 ① 竖放物品在放置引擎里被当成横放（占位算错）
/// ② 旋转后把 `"r": true` 写进档里，与游戏约定不一致。两种写法都认。
pub(crate) fn is_vertical(loc: &Value) -> bool {
    match loc.get("r") {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => s.eq_ignore_ascii_case("vertical"),
        _ => false,
    }
}

/// 按存档已有写法落 `r`：字符串就用 `"Vertical"` / `"Horizontal"`，布尔就写 bool，
/// 原来没有 r 的一律写字符串（与真实存档一致）。
pub(crate) fn set_vertical(loc: &mut Value, vertical: bool) {
    let Some(obj) = loc.as_object_mut() else {
        return;
    };
    match obj.get("r") {
        Some(Value::Bool(_)) => {
            obj.insert("r".into(), json!(vertical));
        }
        _ => {
            obj.insert(
                "r".into(),
                json!(if vertical { "Vertical" } else { "Horizontal" }),
            );
        }
    }
}

pub(crate) fn find_free_slot(
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
        .pointer("/characters/pmc")
        .map(stash_rows_bonus)
        .unwrap_or(0);

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
        let r = is_vertical(loc);
        let (iw, ih) = db
            .entries
            .get(itpl)
            .map(|p| {
                if r {
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

/// 口袋容器：(物品 id, 模板 tpl)。即 equipment 下 `slotId == "Pockets"` 的那件。
pub(crate) fn pockets_of(raw: &Value) -> Option<(String, String)> {
    let equip = raw
        .pointer("/characters/pmc/Inventory/equipment")
        .and_then(Value::as_str)?;
    let items = raw
        .pointer("/characters/pmc/Inventory/items")
        .and_then(Value::as_array)?;
    let it = items.iter().find(|i| {
        i.get("parentId").and_then(Value::as_str) == Some(equip)
            && i.get("slotId").and_then(Value::as_str) == Some("Pockets")
    })?;
    Some((
        it.get("_id")?.as_str()?.to_string(),
        it.get("_tpl")?.as_str()?.to_string(),
    ))
}

/// 口袋格名：口袋模板 `Grids` 逐个 1×1，按 SPT 存盘约定命名 `pocket1..pocketN`
/// （真实档里口袋物品的 `slotId` 正是 `pocket1` / `pocket4`）。
pub(crate) fn pocket_cell_names(db_dir: &str, pockets_tpl: &str) -> Result<Vec<String>, String> {
    let txt = fs::read_to_string(Path::new(db_dir).join("templates").join("items.json"))
        .map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&txt).map_err(|e| e.to_string())?;
    let grids = v
        .get(pockets_tpl)
        .and_then(|t| t.pointer("/_props/Grids"))
        .and_then(|g| g.as_array())
        .ok_or("口袋模板没有网格")?;
    Ok((0..grids.len()).map(|i| format!("pocket{}", i + 1)).collect())
}

/// 往口袋放一件物品：占第一个空格；口袋格是 1×1，只接受 1×1 物品，满了报错不静默丢弃
pub(crate) fn add_item_to_pockets(
    raw: &mut Value,
    pocket_id: &str,
    tpl: &str,
    db_dir: &str,
    db: &ItemsDb,
    seq: u32,
) -> Result<(), String> {
    let props = db
        .entries
        .get(tpl)
        .ok_or_else(|| format!("未知物品模板: {}", tpl))?;
    if props.width.max(1) != 1 || props.height.max(1) != 1 {
        return Err("这件物品占多个格，口袋放不进去".into());
    }
    let (_, pockets_tpl) = pockets_of(raw).ok_or("当前档案没有口袋")?;
    let cells = pocket_cell_names(db_dir, &pockets_tpl)?;
    let used: Vec<String> = raw
        .pointer("/characters/pmc/Inventory/items")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter(|i| i.get("parentId").and_then(Value::as_str) == Some(pocket_id))
                .filter_map(|i| i.get("slotId").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let free = cells
        .iter()
        .find(|c| !used.contains(c))
        .cloned()
        .ok_or("四个口袋都装满了")?;
    let stack_max = props.stack_max.max(1);
    let items = raw
        .pointer_mut("/characters/pmc/Inventory/items")
        .and_then(|v| v.as_array_mut())
        .ok_or("Inventory.items 缺失")?;
    let mut new_it = json!({
        "_id": new_item_id(seq.wrapping_add(items.len() as u32)),
        "_tpl": tpl,
        "parentId": pocket_id,
        "slotId": free,
        "location": { "x": 0, "y": 0, "r": "Horizontal" }
    });
    if stack_max > 1 {
        new_it["upd"] = json!({ "StackObjectsCount": 1 });
    }
    items.push(new_it);
    Ok(())
}

/// 口袋内换位：目标格空着就挪过去，有物品就两者换格（只换 slotId / location）
pub(crate) fn move_in_pocket(
    raw: &mut Value,
    pocket_id: &str,
    id: &str,
    slot: &str,
    db_dir: &str,
) -> Result<(), String> {
    let (_, pockets_tpl) = pockets_of(raw).ok_or("当前档案没有口袋")?;
    let cells = pocket_cell_names(db_dir, &pockets_tpl)?;
    if !cells.iter().any(|c| c == slot) {
        return Err("没有这个口袋格".into());
    }
    let items = raw
        .pointer_mut("/characters/pmc/Inventory/items")
        .and_then(|v| v.as_array_mut())
        .ok_or("Inventory.items 缺失")?;
    let in_pocket = |it: &Value| it.get("parentId").and_then(Value::as_str) == Some(pocket_id);
    let from = items
        .iter()
        .position(|i| in_pocket(i) && i.get("_id").and_then(Value::as_str) == Some(id))
        .ok_or("物品不在口袋里")?;
    let src_slot = items[from]
        .get("slotId")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if src_slot == slot {
        return Ok(());
    }
    let to = items.iter().position(|i| {
        in_pocket(i) && i.get("slotId").and_then(Value::as_str) == Some(slot)
    });
    match to {
        Some(to) => {
            items[to]["slotId"] = json!(src_slot);
            items[from]["slotId"] = json!(slot);
        }
        None => {
            items[from]["slotId"] = json!(slot);
            items[from]["location"] = json!({ "x": 0, "y": 0, "r": "Horizontal" });
        }
    }
    Ok(())
}

/// 口袋取出到仓库：走仓库放置引擎定位，取出后 parentId=仓库、slotId=hideout
pub(crate) fn pocket_take_out(
    raw: &mut Value,
    stash_id: &str,
    pocket_id: &str,
    id: &str,
    db_dir: &str,
    db: &ItemsDb,
) -> Result<(), String> {
    let tpl = raw
        .pointer("/characters/pmc/Inventory/items")
        .and_then(Value::as_array)
        .and_then(|arr| {
            arr.iter().find(|i| {
                i.get("parentId").and_then(Value::as_str) == Some(pocket_id)
                    && i.get("_id").and_then(Value::as_str) == Some(id)
            })
        })
        .and_then(|i| i.get("_tpl").and_then(Value::as_str))
        .ok_or("物品不在口袋里")?
        .to_string();
    let (x, y, rot) = find_free_slot(raw, stash_id, &tpl, db_dir, db)?;
    let items = raw
        .pointer_mut("/characters/pmc/Inventory/items")
        .and_then(|v| v.as_array_mut())
        .ok_or("Inventory.items 缺失")?;
    let it = items
        .iter_mut()
        .find(|i| i.get("_id").and_then(Value::as_str) == Some(id))
        .ok_or("找不到要取出的物品")?;
    it["parentId"] = json!(stash_id);
    it["slotId"] = json!("hideout");
    it["location"] = json!({
        "x": x,
        "y": y,
        "r": if rot { "Vertical" } else { "Horizontal" }
    });
    Ok(())
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
            obj["location"] = json!({ "x": place_x, "y": place_y, "r": if rotated { "Vertical" } else { "Horizontal" } });
        }
        items.push(obj);
    }
    Ok(())
}

/// 把全局武器预设整树复制进仓库（薄壳：读 globals 取根与物品表后调 place_build_tree）
pub(crate) fn add_build_to_stash(
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
    // 全局武器预设库在 globals 顶层（4.1.6）；条目字段以下划线开头：
    // `_parent` = 根物品 id、`_items` = 物品表（参考 ServerClasses/ItemPreset.cs 的 JsonProperty 映射）
    let preset = crate::character::item_presets(&v)
        .and_then(|p| p.get(preset_id))
        .ok_or("查无该预设")?;
    let Some(root_id) = preset
        .get("_parent")
        .or_else(|| preset.get("root"))
        .and_then(|r| r.as_str())
    else {
        return Err("预设缺根物品".to_string());
    };
    let pitems = preset
        .get("_items")
        .or_else(|| preset.get("items"))
        .and_then(|i| i.as_array())
        .ok_or("预设缺物品表")?;
    place_build_tree(raw, stash_id, root_id, pitems, db_dir, db, seq)
}

/// 仓库网格尺寸：stash 模板 Grids[0]._props 的 cellsV（行）/ cellsH（列），
/// 从 items.json 原始条目回读（ItemsDb 只存了计数，不存格径）。
/// 键名大小写随版本变动：4.1 起为小写 `cellsV/cellsH`，4.0 及更早为大写 `CellsV/CellsH`
pub(crate) fn stash_grid_size(db_dir: &str, stash_tpl: &str) -> Result<(i64, i64), String> {
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
    let dim = |lower: &str, upper: &str| -> i64 {
        g.get(lower)
            .or_else(|| g.get(upper))
            .and_then(|x| x.as_f64())
            .unwrap_or(0.0) as i64
    };
    Ok((dim("cellsV", "CellsV"), dim("cellsH", "CellsH")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 前端发的是 camelCase（`{op:'addBuild', presetId}`），
    /// enum 光加 rename_all 不会重命名字段，必须 rename_all_fields —— 否则报
    /// `missing field 'preset_id'`（v0.2.5 图鉴「加入仓库」事故）。
    #[test]
    fn add_build_op_accepts_camel_case_preset_id() {
        let d: StashSaveData =
            serde_json::from_str(r#"{"ops":[{"op":"addBuild","presetId":"6aeb01f9"}]}"#)
                .expect("camelCase presetId 应能反序列化");
        match d.ops.as_slice() {
            [StashOp::AddBuild { preset_id }] => assert_eq!(preset_id, "6aeb01f9"),
            other => panic!("变体不符: {:?}", other.len()),
        }
    }

    /// 其余变体的字段不受影响
    #[test]
    fn other_ops_still_deserialize() {
        let d: StashSaveData = serde_json::from_str(
            r#"{"ops":[{"op":"addMoney","tpl":"5696686a4bdc2da3298b456a","amount":1000},
                       {"op":"setBonusRows","count":2},
                       {"op":"removeItems","ids":["a"]},
                       {"op":"addItem","tpl":"x"}]}"#,
        )
        .expect("其它 op 应能反序列化");
        assert_eq!(d.ops.len(), 4);
    }

    /// 本机真实库自检（默认忽略）：4.1.6 的 stash 模板键为小写 cellsV/cellsH
    #[test]
    #[ignore = "需要本机 SPT 安装 E:\\EFT"]
    fn real_stash_grid_reads_lowercase_cells() {
        let (rows, cols) = stash_grid_size(
            r"E:\EFT\SPT_Runtime\SPT_Data\database",
            "5811ce772459770e9e5f9532",
        )
        .expect("读取 stash 网格失败");
        assert_eq!((rows, cols), (68, 10), "读到 {}x{}", rows, cols);
    }

    /// 端到端自检（默认忽略）：把真实档第一个武器预设落到 stash，只在内存里做、不写盘。
    /// 覆盖「加入仓库」整条链路：取 Root/Items → 找空位 → 重编 id → 挂树。
    #[test]
    #[ignore = "需要本机 SPT 存档 E:\\EFT"]
    fn real_weapon_build_lands_in_stash() {
        let db_dir = r"E:\EFT\SPT_Runtime\SPT_Data\database";
        let text = std::fs::read_to_string(
            r"E:\EFT\SPT_Runtime\user\profiles\6ae7b434451bc24b48942228.json",
        )
        .expect("读真实存档失败");
        let mut raw: Value = serde_json::from_str(&text).expect("解析真实存档失败");

        let build = raw
            .pointer("/userbuilds/weaponBuilds")
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .expect("没有武器预设")
            .clone();
        let root_id = build
            .get("Root")
            .and_then(|v| v.as_str())
            .expect("缺 Root")
            .to_string();
        let pitems: Vec<Value> = build
            .get("Items")
            .and_then(|v| v.as_array())
            .cloned()
            .expect("缺 Items");
        let stash_id = raw
            .pointer("/characters/pmc/Inventory/stash")
            .and_then(|v| v.as_str())
            .expect("缺 stash")
            .to_string();

        let db = ItemsDb::load_from(Path::new(db_dir), None);
        let raw_old_ids: Vec<String> = raw
            .pointer("/characters/pmc/Inventory/items")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|it| it.get("_id").and_then(|v| v.as_str()).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let before = raw_old_ids.len();

        place_build_tree(&mut raw, &stash_id, &root_id, &pitems, db_dir, &db, 0)
            .expect("落位失败");

        let after_items = raw
            .pointer("/characters/pmc/Inventory/items")
            .and_then(|v| v.as_array())
            .expect("items 缺失");
        assert!(
            after_items.len() >= before + pitems.len(),
            "落位后 items {before} → {}（预设 {} 件）",
            after_items.len(),
            pitems.len()
        );
        let placed = after_items.iter().any(|it| {
            it.get("parentId").and_then(|v| v.as_str()) == Some(stash_id.as_str())
                && it.get("slotId").and_then(|v| v.as_str()) == Some("hideout")
                && it.get("location").is_some()
        });
        assert!(placed, "stash 下没有带坐标的新增根物品");

        // 落位后新根恰好一个；预设里每件物品都应落成一条新记录
        let before_ids: Vec<String> = raw_old_ids.clone();
        let new_root = after_items
            .iter()
            .filter(|it| {
                let id = it.get("_id").and_then(|v| v.as_str()).unwrap_or("");
                it.get("parentId").and_then(|v| v.as_str()) == Some(stash_id.as_str())
                    && it.get("slotId").and_then(|v| v.as_str()) == Some("hideout")
                    && !before_ids.iter().any(|b| b == id)
            })
            .count();
        assert_eq!(new_root, 1, "一次加入应只有一个新根，实际 {new_root}");
    }

    /// 端到端自检（默认忽略）：从**全局预设库**（globals 顶层 ItemPresets，_parent/_items 形态）
    /// 取一条落到 stash。此前读 config.ItemPresets + root/items 全取不到 → 图鉴「加入仓库」必失败。
    #[test]
    #[ignore = "需要本机 SPT 存档 E:\\EFT"]
    fn real_global_preset_lands_in_stash() {
        let db_dir = r"E:\EFT\SPT_Runtime\SPT_Data\database";
        let text = std::fs::read_to_string(
            r"E:\EFT\SPT_Runtime\user\profiles\6ae7b434451bc24b48942228.json",
        )
        .expect("读真实存档失败");
        let mut raw: Value = serde_json::from_str(&text).expect("解析真实存档失败");

        let gtxt = std::fs::read_to_string(Path::new(db_dir).join("globals.json")).unwrap();
        let g: Value = serde_json::from_str(&gtxt).unwrap();
        let presets = crate::character::item_presets(&g).expect("全局预设库未解析");
        // 挑一条能落位的：根模板必须在物品库里有格径（这里直接用第一条 AK-74N）
        let (pid, _) = presets
            .iter()
            .find(|(_, p)| p.get("_name").and_then(|v| v.as_str()) == Some("AK-74N"))
            .expect("全局预设里没有 AK-74N");

        let stash_id = raw
            .pointer("/characters/pmc/Inventory/stash")
            .and_then(|v| v.as_str())
            .expect("缺 stash")
            .to_string();
        let db = ItemsDb::load_from(Path::new(db_dir), None);
        let before = raw
            .pointer("/characters/pmc/Inventory/items")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);

        add_build_to_stash(&mut raw, &stash_id, pid, db_dir, &db, 0)
            .expect("全局预设入库失败");

        let after = raw
            .pointer("/characters/pmc/Inventory/items")
            .and_then(|v| v.as_array())
            .expect("items 缺失");
        assert!(after.len() > before, "落位后物品数没有增加");
        let placed = after.iter().any(|it| {
            it.get("parentId").and_then(|v| v.as_str()) == Some(stash_id.as_str())
                && it.get("slotId").and_then(|v| v.as_str()) == Some("hideout")
                && it.get("location").is_some()
        });
        assert!(placed, "stash 下没有带坐标的新增根物品");
    }

    /// 货币必须能从容器内部算到：玩家常把钱放进钱箱（59fb016586f7746d0d4b423a）。
    #[test]
    fn sum_money_counts_stacks_inside_containers() {
        let items: Vec<Value> = serde_json::from_str(
            r#"[
              {"_id":"cash1","_tpl":"59fb016586f7746d0d4b423a","parentId":"STASH","slotId":"hideout"},
              {"_id":"m1","_tpl":"5449016a4bdc2d6f028b456f","parentId":"cash1","slotId":"main","upd":{"StackObjectsCount":1000000}},
              {"_id":"m2","_tpl":"5449016a4bdc2d6f028b456f","parentId":"cash1","slotId":"main","upd":{"StackObjectsCount":706323}},
              {"_id":"m3","_tpl":"5449016a4bdc2d6f028b456f","parentId":"STASH","slotId":"hideout","upd":{"StackObjectsCount":5000}},
              {"_id":"d1","_tpl":"5696686a4bdc2da3298b456a","parentId":"cash1","slotId":"main","upd":{"StackObjectsCount":935}}
            ]"#,
        )
        .unwrap();
        assert_eq!(sum_money(&items, RUBLE_TPL), 1_711_323);
        assert_eq!(sum_money(&items, DOLLAR_TPL), 935);
        assert_eq!(sum_money(&items, EURO_TPL), 0);
    }

    /// 本机真实档自检（默认忽略）：钱在钱箱里也必须算出来，不能是 0。
    #[test]
    #[ignore = "需要本机 SPT 存档 E:\\EFT"]
    fn real_profile_money_is_counted() {
        let dir = r"E:\EFT\SPT_Runtime\user\profiles";
        let mut found = None;
        for e in std::fs::read_dir(dir).expect("读 profiles 失败").flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            let txt = std::fs::read_to_string(&p).unwrap();
            let v: Value = serde_json::from_str(&txt).unwrap();
            let items = v
                .pointer("/characters/pmc/Inventory/items")
                .and_then(|x| x.as_array());
            if let Some(items) = items {
                let r = sum_money(items, RUBLE_TPL);
                if r > 0 {
                    found = Some((p.display().to_string(), r));
                    break;
                }
            }
        }
        let (path, rubles) = found.expect("没有任何存档能算出卢布");
        assert!(rubles > 1_000_000, "{} 只算出 {} 卢布", path, rubles);
    }

    /// 本机真实档自检（默认忽略）：仓库顶层物品必须带格位与模板尺寸 ——
    /// 「格子视图」靠这两个字段画，缺一个就会全挤在 (0,0)。
    #[test]
    #[ignore = "需要本机 SPT 存档 E:\\EFT"]
    fn real_stash_rows_carry_location_and_size() {
        let db_dir = r"E:\EFT\SPT_Runtime\SPT_Data\database";
        let db = ItemsDb::load_from(Path::new(db_dir), None);
        let dir = r"E:\EFT\SPT_Runtime\user\profiles";
        let mut checked = 0usize;
        for e in std::fs::read_dir(dir).expect("读 profiles 失败").flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            let txt = std::fs::read_to_string(&p).unwrap();
            let v: Value = serde_json::from_str(&txt).unwrap();
            let Some(items) = v
                .pointer("/characters/pmc/Inventory/items")
                .and_then(|x| x.as_array())
            else {
                continue;
            };
            let stash_id = v
                .pointer("/characters/pmc/Inventory/stash")
                .and_then(|x| x.as_str())
                .unwrap_or("");
            for it in items {
                if it.get("parentId").and_then(|x| x.as_str()) != Some(stash_id) {
                    continue;
                }
                let row = item_row(
                    it.get("_id").and_then(|x| x.as_str()).unwrap_or(""),
                    it.get("_tpl").and_then(|x| x.as_str()).unwrap_or(""),
                    Some(it),
                    &db,
                );
                assert!(row.x >= 0 && row.y >= 0, "{} 缺格位: x={} y={}", row.name, row.x, row.y);
                assert!(row.w >= 1 && row.h >= 1, "{} 尺寸异常: {}×{}", row.name, row.w, row.h);
                // 旋转时占格要交换，否则画出来会压到相邻格
                let (cw, ch) = if row.rot { (row.h, row.w) } else { (row.w, row.h) };
                assert!(cw <= 10, "{} 横向占 {} 格，超过仓库 10 列", row.name, cw);
                assert!(ch <= 68, "{} 纵向占 {} 格，超过仓库 68 行", row.name, ch);
                checked += 1;
            }
            if checked > 0 {
                break;
            }
        }
        assert!(checked > 0, "没有任何存档的仓库能取到格位");
    }

    /// 图鉴「加入」的数量字段：camelCase 的 count 能解出，省略时为 None（按 1 件处理）    // ———————— MoveItem 拖动落位 ————————

    fn db_with(sizes: &[(&str, i64, i64)]) -> ItemsDb {
        let mut entries = HashMap::new();
        for (tpl, w, h) in sizes {
            entries.insert(
                (*tpl).to_string(),
                ItemProps {
                    width: *w,
                    height: *h,
                    stack_max: 1,
                    grids: 0,
                    recoil_force_up: 0.0,
                    short_name: String::new(),
                },
            );
        }
        ItemsDb { entries, names: None }
    }

    fn stash_fixture() -> Value {
        json!({
            "characters": { "pmc": { "Inventory": { "items": [
                { "_id": "STASH", "_tpl": "5811ce772459770e9e5f9532" },
                { "_id": "A", "_tpl": "T_W2H2", "parentId": "STASH",
                  "location": { "x": 0, "y": 0, "r": false, "isSearched": true } },
                { "_id": "B", "_tpl": "T_W1H1", "parentId": "STASH",
                  "location": { "x": 5, "y": 5, "r": false } },
                { "_id": "BOX", "_tpl": "T_W1H1", "parentId": "STASH",
                  "location": { "x": 8, "y": 8, "r": false } },
                { "_id": "INBOX", "_tpl": "T_W1H1", "parentId": "BOX" }
            ] } } }
        })
    }

    #[test]
    fn move_rejects_container_child() {
        let mut raw = stash_fixture();
        let db = db_with(&[("T_W1H1", 1, 1), ("T_W2H2", 2, 2)]);
        let err = move_item_in_stash(&mut raw, "STASH", "INBOX", 1, 1, false, "", &db).unwrap_err();
        assert!(err.contains("容器"), "应提示容器内物品不能直接移动，实际：{}", err);
    }

    #[test]
    fn move_refuses_when_grid_unknown() {
        // 读不到仓库网格尺寸时必须拒绝，而不是当成 0×0 硬写
        let db = db_with(&[("T_W1H1", 1, 1), ("T_W2H2", 2, 2)]);
        let mut raw = stash_fixture();
        let err = move_item_in_stash(&mut raw, "STASH", "B", 3, 3, false, "", &db);
        assert!(err.is_err(), "缺少网格信息时应拒绝落位");
    }

    #[test]
    fn move_keeps_other_fields_untouched() {
        let mut raw = stash_fixture();
        let db = db_with(&[("T_W1H1", 1, 1), ("T_W2H2", 2, 2)]);
        // 用一个孤立物品验证 location 之外的字段不被改写
        let before = raw["characters"]["pmc"]["Inventory"]["items"][1].clone();
        let _ = move_item_in_stash(&mut raw, "STASH", "A", 0, 0, false, "", &db);
        let after = &raw["characters"]["pmc"]["Inventory"]["items"][1];
        assert_eq!(before["_tpl"], after["_tpl"]);
        assert_eq!(before["parentId"], after["parentId"]);
        assert_eq!(before["location"]["isSearched"], after["location"]["isSearched"]);
    }

    /// 测试用：递归收集两份 JSON 的差异路径（只读比对）
    fn diff_paths_local(a: &Value, b: &Value, base: &str, out: &mut Vec<String>) {
        match (a, b) {
            (Value::Object(ma), Value::Object(mb)) => {
                let mut keys: Vec<&String> = ma.keys().chain(mb.keys()).collect();
                keys.sort();
                keys.dedup();
                for k in keys {
                    let p = format!("{}/{}", base, k);
                    match (ma.get(k), mb.get(k)) {
                        (Some(va), Some(vb)) => diff_paths_local(va, vb, &p, out),
                        _ => out.push(p),
                    }
                }
            }
            (Value::Array(va), Value::Array(vb)) if va.len() == vb.len() => {
                for (i, (x, y)) in va.iter().zip(vb.iter()).enumerate() {
                    diff_paths_local(x, y, &format!("{}/{}", base, i), out);
                }
            }
            _ => {
                if a != b {
                    out.push(base.to_string());
                }
            }
        }
    }

    #[test]
    #[ignore = "需要本机 SPT 存档 E:\\EFT"]
    fn real_move_item_in_place_is_byte_stable() {
        let dir = std::path::Path::new(r"E:\EFT\SPT_Runtime\user\profiles");
        let mut files: Vec<_> = std::fs::read_dir(dir)
            .expect("读不到存档目录")
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
            .collect();
        files.sort();
        let db_dir = r"E:\EFT\SPT_Runtime\SPT_Data\database";
        let db = ItemsDb::load_from(std::path::Path::new(db_dir), None);
        for path in files {
            let txt = std::fs::read_to_string(&path).expect("读档失败");
            let mut raw: Value = serde_json::from_str(&txt).expect("解析失败");
            let stash_id = raw
                .pointer("/characters/pmc/Inventory/stash")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if stash_id.is_empty() {
                continue;
            }
            // 找一件直接挂在仓库下、带 location 的物品
            let pick = raw
                .pointer("/characters/pmc/Inventory/items")
                .and_then(|v| v.as_array())
                .and_then(|arr| {
                    arr.iter().find(|it| {
                        it.get("parentId").and_then(|v| v.as_str()) == Some(stash_id.as_str())
                            && it.get("location").is_some()
                    })
                })
                .cloned();
            let Some(item) = pick else { continue };
            let id = item.get("_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let lx = item.pointer("/location/x").and_then(|v| v.as_i64()).unwrap_or(0);
            let ly = item.pointer("/location/y").and_then(|v| v.as_i64()).unwrap_or(0);
            let lr = item.pointer("/location/r").and_then(|v| v.as_bool()).unwrap_or(false);
            let before = raw.clone();
            // 原地移动：坐标不变，应当成功且不产生任何字节差异
            let got = move_item_in_stash(&mut raw, &stash_id, &id, lx, ly, lr, db_dir, &db);
            assert!(got.is_ok(), "原地移动被拒：{:?}", got.err());
            let mut diffs = Vec::new();
            diff_paths_local(&before, &raw, "", &mut diffs);
            assert!(diffs.is_empty(), "原地移动改动了别的字段：{:?}", diffs);
            return;
        }
        panic!("没找到可测试的仓库物品");
    }

    #[test]
    fn add_item_accepts_optional_count() {
        let with: StashSaveData = serde_json::from_str(
            r#"{"ops":[{"op":"addItem","tpl":"5449016a4bdc2d6f028b456f","count":5}]}"#,
        )
        .unwrap();
        match &with.ops[0] {
            StashOp::AddItem { count, .. } => assert_eq!(*count, Some(5)),
            other => panic!("解成了 {:?}", other),
        }
        let without: StashSaveData = serde_json::from_str(
            r#"{"ops":[{"op":"addItem","tpl":"5449016a4bdc2d6f028b456f"}]}"#,
        )
        .unwrap();
        match &without.ops[0] {
            StashOp::AddItem { count, .. } => assert_eq!(*count, None),
            other => panic!("解成了 {:?}", other),
        }
    }

    /// 本机真实档自检（默认忽略）：一次加 12 件可堆叠小件，总量必须正好多 12。
    #[test]
    #[ignore = "需要本机 SPT 存档 E:\\EFT"]
    fn real_add_item_honours_count() {
        let db_dir = r"E:\EFT\SPT_Runtime\SPT_Data\database";
        let mut raw: Value = {
            let dir = r"E:\EFT\SPT_Runtime\user\profiles";
            let mut got = None;
            for e in std::fs::read_dir(dir).expect("读 profiles 失败").flatten() {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()) != Some("json") {
                    continue;
                }
                if let Ok(t) = std::fs::read_to_string(&p) {
                    if let Ok(v) = serde_json::from_str::<Value>(&t) {
                        if v.pointer("/characters/pmc/Inventory/stash").is_some() {
                            got = Some(v);
                            break;
                        }
                    }
                }
            }
            got.expect("没有可用存档")
        };
        let db = ItemsDb::load_from(Path::new(db_dir), None);
        // 挑一个可堆叠、1×1 的模板，保证仓库放得下 12 件
        let tpl = db
            .entries
            .iter()
            .find(|(_, p)| p.stack_max >= 8 && p.width == 1 && p.height == 1)
            .map(|(k, _)| k.clone())
            .expect("物品库里没有 1×1 可堆叠物品");
        let stash_id = raw
            .pointer("/characters/pmc/Inventory/stash")
            .and_then(|v| v.as_str())
            .expect("缺 stash")
            .to_string();

        let count_in = |v: &Value| -> i64 {
            v.pointer("/characters/pmc/Inventory/items")
                .and_then(|x| x.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter(|it| {
                            it.get("parentId").and_then(Value::as_str) == Some(stash_id.as_str())
                                && it.get("_tpl").and_then(Value::as_str) == Some(tpl.as_str())
                        })
                        .map(|it| {
                            it.pointer("/upd/StackObjectsCount")
                                .and_then(Value::as_i64)
                                .unwrap_or(1)
                        })
                        .sum()
                })
                .unwrap_or(0)
        };
        let before = count_in(&raw);
        add_item_to_stash(&mut raw, &stash_id, &tpl, db_dir, &db, 0, 12).expect("加物品失败");
        let after = count_in(&raw);
        assert_eq!(after - before, 12, "{} 加 12 件后差 {}", tpl, after - before);
    }
}
