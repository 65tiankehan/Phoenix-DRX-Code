//! 角色页（03-character）数据服务 —— 读全量 / 选项 / 写回。
//! 结构对照参考编辑器（ProfileSaver.cs / AppData.cs / CharacterInventory.cs）：
//! - 体征：Health.BodyParts.{部位}.Health.{Current,Maximum}（七部位），
//!   Energy/Hydration 直接挂在 Health 下（无内层 Health）；
//! - 语音/头部选项：扫 <database>/bots/types/*.json 的 appearance.voice/head
//!   对象键，名字取 <database>/locales/global/en.json 的「{key} Name」，
//!   缺失兜底「{文件名} [{key}]」；
//! - 口袋选项：templates/items.json 中 _parent = 557596e6… 的物品，
//!   槽数 = 各 Grid._props 的 CellsH×CellsV 之和（任一 Grid._props 缺失则 0），
//!   特殊槽 = _props.Slots 中 _name 以 SpecialSlot 开头的计数；
//! - 写回铁律：先备份，后定点替换；数值一律强转整数；口袋仅替换已有物品
//!   的 _tpl（不存在不新建）；阵营仅 PMC 写 Info.Side。

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::AppHandle;

use crate::backup::backup_before_write;
use crate::profile::{is_safe_id, profiles_dir, SaveResult};
use crate::server::current_server;

/// 口袋物品的 _parent 判定 id（SPT DefaultValues.cs 权威值）
const POCKETS_PARENT: &str = "557596e64bdc2dc2118b4571";
/// 口袋在 Inventory.items 中的槽位名
const POCKET_SLOT_ID: &str = "Pockets";

// ———————— 类型 ————————

/// 单项生命体征（当前值 / 上限值）
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CharVital {
    pub current: i64,
    pub maximum: i64,
}

/// 九项体征：七部位 + 水分 + 能量
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CharHealth {
    pub head: CharVital,
    pub chest: CharVital,
    pub stomach: CharVital,
    pub left_arm: CharVital,
    pub right_arm: CharVital,
    pub left_leg: CharVital,
    pub right_leg: CharVital,
    pub hydration: CharVital,
    pub energy: CharVital,
}

/// 单侧角色可编辑数据（PMC / Scav 对称）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CharSideData {
    /// 阵营（Bear/Usec）；仅 PMC 写回 Info.Side，Scav 固定 Savage 不触碰
    pub side: String,
    pub nickname: String,
    pub level: i64,
    pub experience: i64,
    pub head: String,
    pub voice: String,
    pub health: CharHealth,
    pub pockets_tpl: String,
}

/// 角色页全量数据：pmc/scav + 顶部只读区
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CharData {
    pub side: String,
    pub aid: String,
    pub game_version: String,
    pub pmc: CharSideData,
    pub scav: CharSideData,
}

/// 下拉选项（id + 展示名）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CharOptionItem {
    pub id: String,
    pub name: String,
}

/// 口袋选项；name 按参考 GetPocketsName 拼好：「名字 (槽数) [+ special slots (特殊槽数)]」
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CharPocketItem {
    pub id: String,
    pub name: String,
    pub slots: i64,
    pub special_slots: i64,
}

/// 角色页选项集：经验增量表 / 语音 / 头部 / 口袋
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CharOptions {
    pub exp_table: Vec<i64>,
    pub voices: Vec<CharOptionItem>,
    pub heads: Vec<CharOptionItem>,
    pub pockets: Vec<CharPocketItem>,
}

/// 保存载荷（双侧全量；阵营仅 PMC 生效）
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CharSaveData {
    pub pmc: CharSideData,
    pub scav: CharSideData,
}

// ———————— 读侧 helpers ————————

/// 当前服务器 database 根目录：<root>\SPT\SPT_Data\database
pub(crate) fn database_dir(app: &AppHandle) -> Option<String> {
    Some(format!(
        "{}\\SPT\\SPT_Data\\database",
        current_server(app)?.path.trim_end_matches(['\\', '/'])
    ))
}

/// 部位体征：{part}.Health.{Current,Maximum}
fn vital_in(part: Option<&Value>) -> CharVital {
    let hp = part.and_then(|p| p.get("Health"));
    CharVital {
        current: hp
            .and_then(|h| h.get("Current"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as i64,
        maximum: hp
            .and_then(|h| h.get("Maximum"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as i64,
    }
}

/// 直挂体征：{stat}.{Current,Maximum}（Energy / Hydration 无内层 Health）
fn vital_direct(stat: Option<&Value>) -> CharVital {
    CharVital {
        current: stat
            .and_then(|s| s.get("Current"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as i64,
        maximum: stat
            .and_then(|s| s.get("Maximum"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as i64,
    }
}

fn read_health(ch: Option<&Value>) -> CharHealth {
    let body = ch.and_then(|h| h.get("BodyParts"));
    CharHealth {
        head: vital_in(body.and_then(|b| b.get("Head"))),
        chest: vital_in(body.and_then(|b| b.get("Chest"))),
        stomach: vital_in(body.and_then(|b| b.get("Stomach"))),
        left_arm: vital_in(body.and_then(|b| b.get("LeftArm"))),
        right_arm: vital_in(body.and_then(|b| b.get("RightArm"))),
        left_leg: vital_in(body.and_then(|b| b.get("LeftLeg"))),
        right_leg: vital_in(body.and_then(|b| b.get("RightLeg"))),
        hydration: vital_direct(ch.and_then(|h| h.get("Hydration"))),
        energy: vital_direct(ch.and_then(|h| h.get("Energy"))),
    }
}

/// 口袋物品模板 id：Inventory.items 中 slotId == "Pockets" 的 _tpl（无则空串）
fn pockets_tpl_of(ch: &Value) -> String {
    ch.get("Inventory")
        .and_then(|i| i.get("items"))
        .and_then(|v| v.as_array())
        .and_then(|items| {
            items
                .iter()
                .find(|it| it.get("slotId").and_then(|s| s.as_str()) == Some(POCKET_SLOT_ID))
        })
        .and_then(|it| it.get("_tpl"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn read_side(ch: Option<&Value>) -> CharSideData {
    let info = ch.and_then(|c| c.get("Info"));
    let cust = ch.and_then(|c| c.get("Customization"));
    CharSideData {
        side: info
            .and_then(|i| i.get("Side"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        nickname: info
            .and_then(|i| i.get("Nickname"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        level: info
            .and_then(|i| i.get("Level"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as i64,
        experience: info
            .and_then(|i| i.get("Experience"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as i64,
        head: cust
            .and_then(|c| c.get("Head"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        // 语音：Customization.Voice 优先，缺失回退 Info.Voice
        voice: cust
            .and_then(|c| c.get("Voice"))
            .and_then(|v| v.as_str())
            .or_else(|| info.and_then(|i| i.get("Voice")).and_then(|v| v.as_str()))
            .unwrap_or("")
            .to_string(),
        health: read_health(ch),
        pockets_tpl: ch.map(pockets_tpl_of).unwrap_or_default(),
    }
}

// ———————— 命令实现 ————————

/// 角色页全量数据；档案缺失或不可读时为 None
pub fn load_char(app: &AppHandle, id: &str) -> Option<CharData> {
    if !is_safe_id(id) {
        return None;
    }
    let dir = profiles_dir(app)?;
    let raw: Value =
        serde_json::from_str(&fs::read_to_string(Path::new(&dir).join(format!("{}.json", id))).ok()?)
            .ok()?;
    let pmc = raw.pointer("/characters/pmc");
    let info = pmc.and_then(|p| p.get("Info"));
    Some(CharData {
        side: info
            .and_then(|i| i.get("Side"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        // 真实档案 aid 挂角色根（Character.cs / 样本 4758 行），不在 Info 下；兼容数字形态
        aid: pmc
            .and_then(|p| p.get("aid"))
            .map(|v| match v {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                _ => String::new(),
            })
            .unwrap_or_default(),
        game_version: info
            .and_then(|i| i.get("GameVersion"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        pmc: read_side(pmc),
        scav: read_side(raw.pointer("/characters/scav")),
    })
}

/// 名字查找：locales/global/en.json 的「{key} Name」，缺失兜底「{来源文件名} [{key}]」
fn localized_name(names: Option<&Value>, key: &str, fallback_file: &str) -> String {
    names
        .and_then(|n| n.get(format!("{} Name", key)))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{} [{}]", fallback_file, key))
}

/// 特殊槽头名：locale 键兜底 "special slots"（AppData.cs GetPocketsName 基准）
fn special_slots_header(names: Option<&Value>) -> String {
    names
        .and_then(|n| n.get("InventoryScreen/SpecialSlotsHeader"))
        .and_then(|v| v.as_str())
        .unwrap_or("special slots")
        .to_string()
}

/// 槽数口径（TarkovItem.cs SlotsCount）：仅当每个 Grid 的 _props 都非空才累计
/// CellsH×CellsV；任一缺失（或无 Grid）按 0 计。
fn slots_count(item: &Value) -> i64 {
    let grids = item
        .pointer("/_props/Grids")
        .and_then(|g| g.as_array())
        .map(|g| g.as_slice())
        .unwrap_or(&[]);
    let mut sum = 0i64;
    for g in grids {
        match g.get("_props") {
            Some(props) if !props.is_null() => {
                let h = props.get("CellsH").and_then(|v| v.as_i64()).unwrap_or(0);
                let v = props.get("CellsV").and_then(|c| c.as_i64()).unwrap_or(0);
                sum += h * v;
            }
            _ => return 0,
        }
    }
    sum
}

/// 特殊槽数：_props.Slots 中 _name 以 "SpecialSlot" 开头的条目数
fn special_slots_count(item: &Value) -> i64 {
    item.pointer("/_props/Slots")
        .and_then(|s| s.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|s| {
                    s.get("_name")
                        .and_then(|n| n.as_str())
                        .map(|n| n.starts_with("SpecialSlot"))
                        .unwrap_or(false)
                })
                .count() as i64
        })
        .unwrap_or(0)
}

/// 角色页选项集。database 缺失时各字段为空集，不阻塞页面
pub fn load_options(app: &AppHandle) -> CharOptions {
    let mut opts = CharOptions::default();
    let Some(db) = database_dir(app) else {
        return opts;
    };
    let db = Path::new(&db);

    // 经验增量表：globals.json 的 config.exp.level.exp_table
    if let Ok(txt) = fs::read_to_string(db.join("globals.json")) {
        if let Ok(v) = serde_json::from_str::<Value>(&txt) {
            opts.exp_table = v
                .pointer("/config/exp/level/exp_table")
                .and_then(|t| t.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_i64()).collect())
                .unwrap_or_default();
        }
    }

    // 名字查找表：locales/global/en.json
    let names = fs::read_to_string(db.join("locales").join("global").join("en.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok());

    // 语音 / 头部：合并 bots/types/*.json 的 appearance 对象键
    let mut voices: BTreeMap<String, String> = BTreeMap::new();
    let mut heads: BTreeMap<String, String> = BTreeMap::new();
    if let Ok(entries) = fs::read_dir(db.join("bots").join("types")) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            let is_json = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("json"))
                .unwrap_or(false);
            if !is_json {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let Ok(txt) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(v) = serde_json::from_str::<Value>(&txt) else {
                continue;
            };
            let Some(appear) = v.get("appearance") else {
                continue;
            };
            // voice/head 均为「键即 id」的对象（Dictionary<string, T>）
            for (group, map) in [("voice", &mut voices), ("head", &mut heads)] {
                if let Some(obj) = appear.get(group).and_then(|g| g.as_object()) {
                    for key in obj.keys() {
                        map.entry(key.clone()).or_insert_with(|| {
                            localized_name(names.as_ref(), key, stem)
                        });
                    }
                }
            }
        }
    }
    opts.voices = voices
        .into_iter()
        .map(|(id, name)| CharOptionItem { id, name })
        .collect();
    opts.heads = heads
        .into_iter()
        .map(|(id, name)| CharOptionItem { id, name })
        .collect();
    opts.voices.sort_by(|a, b| a.name.cmp(&b.name));
    opts.heads.sort_by(|a, b| a.name.cmp(&b.name));

    // 口袋：templates/items.json 过滤 _parent，按槽数升序（AppData.cs FindPockets 基准）
    // 真实 items.json 是 { tpl -> TarkovItem } 字典
    if let Ok(txt) = fs::read_to_string(db.join("templates").join("items.json")) {
        if let Ok(v) = serde_json::from_str::<Value>(&txt) {
            if let Some(items) = v.as_object() {
                let mut pockets: Vec<CharPocketItem> = items
                    .values()
                    .filter(|it| {
                        it.get("_parent").and_then(|p| p.as_str()) == Some(POCKETS_PARENT)
                    })
                    .filter_map(|it| {
                        let id = it.get("_id").and_then(|i| i.as_str())?;
                        let slots = slots_count(it);
                        let special = special_slots_count(it);
                        let local =
                            names
                                .as_ref()
                                .and_then(|n| n.get(format!("{} Name", id)))
                                .and_then(|x| x.as_str())
                                .unwrap_or(id)
                                .to_string();
                        let mut name = format!("{} ({})", local, slots);
                        if special > 0 {
                            name += &format!(" + {} ({})", special_slots_header(names.as_ref()), special);
                        }
                        Some(CharPocketItem {
                            id: id.to_string(),
                            name,
                            slots,
                            special_slots: special,
                        })
                    })
                    .collect();
                pockets.sort_by_key(|p| p.slots);
                opts.pockets = pockets;
            }
        }
    }
    opts
}

// ———————— 写回 ————————

/// 单侧定点替换（ProfileSaver.cs 写回段基准）：
/// 昵称（含小写副本）/ 等级 / 经验 / 阵营（仅 PMC）/ 外观 / 体征 / 口袋；
/// 数值一律强转整数。
fn write_side(side: Option<&mut Value>, data: &CharSideData, is_pmc: bool) {
    let Some(side) = side else { return };
    let Some(info) = side.get_mut("Info") else { return };
    // 阵营：仅 PMC 写 Info.Side（Scav 固定 Savage，不触碰）
    if is_pmc && !data.side.is_empty() {
        info["Side"] = json!(data.side);
    }
    info["Nickname"] = json!(data.nickname);
    info["LowerNickname"] = json!(data.nickname.to_lowercase());
    info["Level"] = json!(data.level.max(0));
    info["Experience"] = json!(data.experience.max(0));

    // 外观：Customization 缺失则建；空值不写，避免塞空串覆盖
    if !data.head.is_empty() || !data.voice.is_empty() {
        if side.get_mut("Customization").is_none() {
            side["Customization"] = json!({});
        }
        if let Some(cust) = side.get_mut("Customization") {
            if !data.head.is_empty() {
                cust["Head"] = json!(data.head);
            }
            if !data.voice.is_empty() {
                cust["Voice"] = json!(data.voice);
            }
        }
    }

    let Some(health) = side.get_mut("Health") else { return };
    let Some(body) = health.get_mut("BodyParts") else { return };
    for (key, vital) in [
        ("Head", data.health.head),
        ("Chest", data.health.chest),
        ("Stomach", data.health.stomach),
        ("LeftArm", data.health.left_arm),
        ("RightArm", data.health.right_arm),
        ("LeftLeg", data.health.left_leg),
        ("RightLeg", data.health.right_leg),
    ] {
        if let Some(hp) = body.get_mut(key).and_then(|p| p.get_mut("Health")) {
            hp["Current"] = json!(vital.current.max(0));
            hp["Maximum"] = json!(vital.maximum.max(0));
        }
    }
    // Energy / Hydration 直挂 Health 下（无内层 Health）
    for (key, vital) in [
        ("Hydration", data.health.hydration),
        ("Energy", data.health.energy),
    ] {
        if let Some(stat) = health.get_mut(key) {
            stat["Current"] = json!(vital.current.max(0));
            stat["Maximum"] = json!(vital.maximum.max(0));
        }
    }

    // 口袋：仅当已有 Pockets 物品时替换其 _tpl，不存在不新建（CharacterInventory.cs 基准）
    if data.pockets_tpl.is_empty() {
        return;
    }
    if let Some(items) = side
        .pointer_mut("/Inventory/items")
        .and_then(|v| v.as_array_mut())
    {
        if let Some(pkt) = items
            .iter_mut()
            .find(|it| it.get("slotId").and_then(|s| s.as_str()) == Some(POCKET_SLOT_ID))
        {
            pkt["_tpl"] = json!(data.pockets_tpl);
        }
    }
}

/// 角色页写回：先强制备份（领域铁律），再定点替换后整体序列化写回
pub fn save_char(app: &AppHandle, id: &str, data: &CharSaveData) -> Result<SaveResult, String> {
    if !is_safe_id(id) {
        return Err(format!("Invalid profile id: {}", id));
    }
    let dir = profiles_dir(app).ok_or_else(|| "未选择 SPT 服务器".to_string())?;
    let path = Path::new(&dir).join(format!("{}.json", id));
    let txt = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut raw: Value = serde_json::from_str(&txt).map_err(|e| e.to_string())?;
    let backup_path = backup_before_write(app, &path)?;

    // 阵营仅 PMC 写 Info.Side；Scav 侧传 is_pmc=false，write_side 不触碰 Info.Side
    write_side(raw.pointer_mut("/characters/pmc"), &data.pmc, true);
    write_side(raw.pointer_mut("/characters/scav"), &data.scav, false);

    let out = serde_json::to_string_pretty(&raw).map_err(|e| e.to_string())?;
    fs::write(&path, out).map_err(|e| e.to_string())?;
    Ok(SaveResult {
        backup_path: backup_path.map(|p| p.to_string_lossy().to_string()),
    })
}
