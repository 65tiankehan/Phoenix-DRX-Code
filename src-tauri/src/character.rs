//! 角色页（03-character）数据服务 —— 读全量 / 选项 / 写回。
//! 结构对照参考编辑器（ProfileSaver.cs / AppData.cs / CharacterInventory.cs）：
//! - 体征：Health.BodyParts.{部位}.Health.{Current,Maximum}（七部位），
//!   Energy/Hydration 直接挂在 Health 下（无内层 Health）；
//! - 语音/头部选项：扫 <database>/bots/types/*.json 的 appearance.voice/head
//!   对象键，名字取 <database>/locales/global/en.json 的「{key} Name」，
//!   缺失兜底「{文件名} [{key}]」；
//! - 口袋选项：templates/items.json 中 _parent = 557596e6… 的物品，
//!   槽数 = 各 Grid._props 的 cellsH×cellsV 之和（4.1 小写；任一 Grid._props 缺失则 0），
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

/// 当前服务器 `SPT_Data` 根目录：`<spt_dir>\SPT_Data`
/// （spt_dir 由 `server::spt_dir` 统一解析：4.1 为 root/SPT_Runtime，4.0 为 root/SPT）
pub(crate) fn spt_data_dir(app: &AppHandle) -> Option<String> {
    let root = crate::server::spt_dir(&current_server(app)?.path);
    Some(root.join("SPT_Data").to_string_lossy().to_string())
}

/// 当前服务器 database 根目录：`<spt_dir>\SPT_Data\database`
pub(crate) fn database_dir(app: &AppHandle) -> Option<String> {
    Some(
        std::path::Path::new(&spt_data_dir(app)?)
            .join("database")
            .to_string_lossy()
            .to_string(),
    )
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

/// 体征：键在角色节点的 `Health` 下（`Health.BodyParts.<部位>.Health`、
/// `Health.Hydration/Energy` 直挂），不是直接挂角色根
fn read_health(ch: Option<&Value>) -> CharHealth {
    let h = ch.and_then(|c| c.get("Health"));
    let body = h.and_then(|x| x.get("BodyParts"));
    CharHealth {
        head: vital_in(body.and_then(|b| b.get("Head"))),
        chest: vital_in(body.and_then(|b| b.get("Chest"))),
        stomach: vital_in(body.and_then(|b| b.get("Stomach"))),
        left_arm: vital_in(body.and_then(|b| b.get("LeftArm"))),
        right_arm: vital_in(body.and_then(|b| b.get("RightArm"))),
        left_leg: vital_in(body.and_then(|b| b.get("LeftLeg"))),
        right_leg: vital_in(body.and_then(|b| b.get("RightLeg"))),
        hydration: vital_direct(h.and_then(|x| x.get("Hydration"))),
        energy: vital_direct(h.and_then(|x| x.get("Energy"))),
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
        // 版本：`spt.version` 才是 SPT 版本（如 "4.1.6 (40743)"）；
        // 4.1 起 `Info.GameVersion` 存的是版本标识（如 edge_of_darkness），只作回退
        game_version: raw
            .pointer("/spt/version")
            .and_then(|v| v.as_str())
            .or_else(|| info.and_then(|i| i.get("GameVersion")).and_then(|v| v.as_str()))
            .unwrap_or("")
            .to_string(),
        pmc: read_side(pmc),
        scav: read_side(raw.pointer("/characters/scav")),
    })
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
                // 4.1 起 Grid 的 _props 用小写 cellsH/cellsV（Alpha container 实测 2×2），
                // 4.0 及更早是大写 CellsH/CellsV —— 只读大写会得到 0 格
                let dim = |lower: &str, upper: &str| -> i64 {
                    props
                        .get(lower)
                        .or_else(|| props.get(upper))
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0)
                };
                sum += dim("cellsH", "CellsH") * dim("cellsV", "CellsV");
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
    // 4.1.6 真实元素是对象 `{exp: N}`（79 项），旧版才是裸数字，两种都要吃；
    // 读成空表会让前端 maxLevel 退化成 1，等级一改就被钳成 1。
    if let Ok(txt) = fs::read_to_string(db.join("globals.json")) {
        if let Ok(v) = serde_json::from_str::<Value>(&txt) {
            opts.exp_table = v
                .pointer("/config/exp/level/exp_table")
                .or_else(|| v.pointer("/config/exp/level/expTable"))
                .and_then(|t| t.as_array())
                .map(|a| {
                    a.iter()
                        .map(|x| {
                            x.get("exp")
                                .and_then(|e| e.as_i64())
                                .or_else(|| x.as_i64())
                                .unwrap_or(0)
                        })
                        .collect()
                })
                .unwrap_or_default();
        }
    }

    // 名字查找表：locales/global/en.json
    let names = fs::read_to_string(db.join("locales").join("global").join("en.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok());

    // 语音 / 头部候选：按当前人物阵营取 bots/types/{usec|bear}.json 的 appearance，
    // 只保留有本地化名的项（4.1.6 实测 USEC = 9 头型 / 7 语音）；
    // 阵营缺失或文件不在时回退全目录聚合。4.1.6 为单数键 head/voice，旧模型为 heads/voices。
    let side = crate::profile::get_current_profile_id(app)
        .and_then(|pid| {
            let dir = profiles_dir(app)?;
            fs::read_to_string(Path::new(&dir).join(format!("{}.json", pid))).ok()
        })
        .and_then(|txt| serde_json::from_str::<Value>(&txt).ok())
        .and_then(|v| {
            v.pointer("/characters/pmc/Info/Side")
                .and_then(|s| s.as_str())
                .map(str::to_string)
        })
        .unwrap_or_default();
    let locale_name = |key: &str| -> Option<String> {
        names
            .as_ref()
            .and_then(|n| n.get(format!("{} Name", key)))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    let types_dir = db.join("bots").join("types");
    let mut sources: Vec<std::path::PathBuf> = Vec::new();
    match side.to_ascii_lowercase().as_str() {
        "usec" => sources.push(types_dir.join("usec.json")),
        "bear" => sources.push(types_dir.join("bear.json")),
        _ => {}
    }
    if sources.is_empty() || sources.iter().any(|p| !p.exists()) {
        sources.clear();
        if let Ok(entries) = fs::read_dir(&types_dir) {
            let mut paths: Vec<_> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.eq_ignore_ascii_case("json"))
                        .unwrap_or(false)
                })
                .collect();
            paths.sort();
            sources = paths;
        }
    }
    let mut voices: BTreeMap<String, String> = BTreeMap::new();
    let mut heads: BTreeMap<String, String> = BTreeMap::new();
    for path in sources {
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
        for (group, alias, map) in [
            ("voice", "voices", &mut voices),
            ("head", "heads", &mut heads),
        ] {
            for slot in [group, alias] {
                let Some(obj) = appear.get(slot).and_then(|g| g.as_object()) else {
                    continue;
                };
                for key in obj.keys() {
                    if map.contains_key(key) {
                        continue;
                    }
                    if let Some(name) = locale_name(key) {
                        map.insert(key.clone(), name);
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
    // 等级最低 1（C# CharacterInfo.Level setter 同样把非法值挡在 1 以上；
    // 写 0 会让客户端读档时按经验反推出 0 级并卡住）
    info["Level"] = json!(data.level.max(1));
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

/// globals.json 的全局武器预设库：4.1.6 真实位置是**顶层** `ItemPresets`（399 条），
/// 早期按 `config.ItemPresets` 写的旧路径一并兼容（否则图鉴 / 检视 / 套装军械库全空）。
pub(crate) fn item_presets(g: &Value) -> Option<&serde_json::Map<String, Value>> {
    g.pointer("/ItemPresets")
        .or_else(|| g.pointer("/config/ItemPresets"))
        .and_then(|p| p.as_object())
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 本机真实库自检（默认忽略）：4.1.6 的全局武器预设库在 globals **顶层**，
    /// 条目字段为 _parent / _items（此前按 config.ItemPresets + root/items 读，全空）
    #[test]
    #[ignore = "需要本机 SPT 安装 E:\\EFT"]
    fn real_globals_exposes_item_presets() {
        let txt = fs::read_to_string(r"E:\EFT\SPT_Runtime\SPT_Data\database\globals.json")
            .expect("读 globals 失败");
        let v: Value = serde_json::from_str(&txt).expect("解析 globals 失败");
        let presets = item_presets(&v).expect("顶层 ItemPresets 未解析出来");
        assert!(presets.len() > 300, "只解析到 {} 条预设", presets.len());
        let sample = presets.values().next().expect("预设为空");
        let root = sample
            .get("_parent")
            .and_then(|x| x.as_str())
            .expect("预设缺 _parent");
        let items = sample
            .get("_items")
            .and_then(|x| x.as_array())
            .expect("预设缺 _items");
        assert!(
            items.iter().any(|i| i.get("_id").and_then(|x| x.as_str()) == Some(root)),
            "_parent 不在 _items 里"
        );
    }

    /// 经验表两种形态：4.1.6 是 `[{exp:N},…]` 对象数组，旧版是裸数字数组。
    /// 读成空表会让前端 maxLevel 退化成 1，等级一改就被钳成 1（真机症状）。
    #[test]
    fn exp_table_reads_object_form() {
        let a: Vec<i64> = serde_json::from_str::<Value>(r#"[{"exp":0},{"exp":1000},{"exp":3017}]"#)
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|x| {
                x.get("exp")
                    .and_then(|e| e.as_i64())
                    .or_else(|| x.as_i64())
                    .unwrap_or(0)
            })
            .collect();
        assert_eq!(a, vec![0, 1000, 3017]);
    }

    /// 本机真实库自检（默认忽略）：exp_table 必须解析出 79 档（上限 78 级）
    #[test]
    #[ignore = "需要本机 SPT 安装 E:\\EFT"]
    fn real_globals_exp_table_has_levels() {
        let txt = fs::read_to_string(r"E:\EFT\SPT_Runtime\SPT_Data\database\globals.json")
            .expect("读 globals 失败");
        let v: Value = serde_json::from_str(&txt).expect("解析 globals 失败");
        let table: Vec<i64> = v
            .pointer("/config/exp/level/exp_table")
            .and_then(|t| t.as_array())
            .expect("exp_table 不是数组")
            .iter()
            .map(|x| {
                x.get("exp")
                    .and_then(|e| e.as_i64())
                    .or_else(|| x.as_i64())
                    .unwrap_or(0)
            })
            .collect();
        assert_eq!(table.len(), 79, "exp_table 档数不对");
        assert_eq!(table[1], 1000);
        assert_eq!(*table.last().unwrap(), 12_000_000);
    }
}
