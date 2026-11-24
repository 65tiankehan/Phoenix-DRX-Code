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
/// 用 f64 而不是整数：EFT 的 Health.Current 是真小数（如 34.649），
/// 早前按 i64 读会让每次保存都把小数截断，回写后玩家血量被悄悄改小。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CharVital {
    pub current: f64,
    pub maximum: f64,
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
    /// 转生等级（`Info.PrestigeLevel`）；仅 PMC 有意义，上限见 `CharData.prestige_max`
    #[serde(default)]
    pub prestige: i64,
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
    /// 转生上限：`templates/prestige.json` 的 elements 数（4.1.6 = 4）；读不到按 0，前端据此禁用编辑
    pub prestige_max: i64,
    /// PMC 已完成的转生记录条数（`pmc.Prestige` 字典长度，只读展示）
    pub prestige_records: i64,
    /// 已准备但尚未执行的转生档位（`spt.pendingPrestige.prestigeLevel`）；None 表示没有待执行的转生
    pub prestige_pending: Option<i64>,
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

/// 转生上限：`templates/prestige.json` 的 `elements` 条数（4.1.6 实测为 4）。
/// 4.1.6 `Server.Core.dll` 的 `PrestigeHelper.ProcessPendingPrestige` 用
/// `Math.Clamp(prestigeLevel - 1, 0, elements.Count - 1)` 取档位，所以上限就是 elements 数。
/// 纯函数便于单测；结构缺失或非法返回 0（前端据此禁用编辑，不猜一个上限出来）。
pub(crate) fn parse_prestige_max(txt: &str) -> i64 {
    serde_json::from_str::<Value>(txt)
        .ok()
        .and_then(|v| {
            v.get("elements")
                .and_then(|e| e.as_array())
                .map(|a| a.len())
        })
        .unwrap_or(0) as i64
}

/// 读当前服务器 `templates/prestige.json` 求转生上限；读不到返回 0
pub(crate) fn prestige_max(app: &AppHandle) -> i64 {
    database_dir(app)
        .and_then(|d| {
            fs::read_to_string(Path::new(&d).join("templates").join("prestige.json")).ok()
        })
        .map(|t| parse_prestige_max(&t))
        .unwrap_or(0)
}

/// 部位体征：{part}.Health.{Current,Maximum}
fn vital_in(part: Option<&Value>) -> CharVital {
    let hp = part.and_then(|p| p.get("Health"));
    CharVital {
        current: hp
            .and_then(|h| h.get("Current"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        maximum: hp
            .and_then(|h| h.get("Maximum"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
    }
}

/// 直挂体征：{stat}.{Current,Maximum}（Energy / Hydration 无内层 Health）
fn vital_direct(stat: Option<&Value>) -> CharVital {
    CharVital {
        current: stat
            .and_then(|s| s.get("Current"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        maximum: stat
            .and_then(|s| s.get("Maximum"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
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
        prestige: info
            .and_then(|i| i.get("PrestigeLevel"))
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
        prestige_max: prestige_max(app),
        // 已完成的转生记录：`pmc.Prestige` 是 `{ "<转生元素 id>": <时间戳> }` 字典，只读展示条数
        prestige_records: pmc
            .and_then(|p| p.get("Prestige"))
            .and_then(|v| v.as_object())
            .map(|m| m.len())
            .unwrap_or(0) as i64,
        // 已准备但尚未执行的转生（`spt.pendingPrestige.prestigeLevel`）：玩家进游戏「重置角色」时由服务器执行
        prestige_pending: raw
            .pointer("/spt/pendingPrestige/prestigeLevel")
            .and_then(|v| v.as_i64()),
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

/// 按原值形态写回数字：原存档是整数且新值无小数就写整数，否则写小数。
/// EFT 的体征既可能是 `35` 也可能是 `34.649`；统一写 f64 会把 `35` 变成 `35.0`，
/// 语义没变但存档被无谓改写，所以这里跟着原形态走。
fn num_like(original: Option<&Value>, v: f64) -> Value {
    let was_int = original.map(|o| o.is_i64() || o.is_u64()).unwrap_or(false);
    if was_int && v.fract() == 0.0 {
        json!(v as i64)
    } else {
        json!(v)
    }
}

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
    // 转生等级只对 PMC 有意义；Scav 侧不触碰 Info.PrestigeLevel（Scav 没有转生）
    if is_pmc {
        info["PrestigeLevel"] = json!(data.prestige.max(0));
    }

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
            let cur_old = hp.get("Current").cloned();
            let max_old = hp.get("Maximum").cloned();
            hp["Current"] = num_like(cur_old.as_ref(), vital.current.max(0.0));
            hp["Maximum"] = num_like(max_old.as_ref(), vital.maximum.max(0.0));
        }
    }
    // Energy / Hydration 直挂 Health 下（无内层 Health）
    for (key, vital) in [
        ("Hydration", data.health.hydration),
        ("Energy", data.health.energy),
    ] {
        if let Some(stat) = health.get_mut(key) {
            let cur_old = stat.get("Current").cloned();
            let max_old = stat.get("Maximum").cloned();
            stat["Current"] = num_like(cur_old.as_ref(), vital.current.max(0.0));
            stat["Maximum"] = num_like(max_old.as_ref(), vital.maximum.max(0.0));
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

/// 转生等级卡范围：上限已知就卡上限，读不到上限只挡负数。
/// 明确不把「读不到上限」当成 0 —— 那会把玩家的转生等级静默清零。
pub(crate) fn clamp_prestige(value: i64, max: i64) -> i64 {
    if max > 0 {
        value.clamp(0, max)
    } else {
        value.max(0)
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
    // 转生等级按 `templates/prestige.json` 的档位数卡上限；读不到上限时只挡负数，绝不静默压 0
    let mut pmc = data.pmc.clone();
    pmc.prestige = clamp_prestige(pmc.prestige, prestige_max(app));
    write_side(raw.pointer_mut("/characters/pmc"), &pmc, true);
    write_side(raw.pointer_mut("/characters/scav"), &data.scav, false);

    // 存档写回一律用**紧凑** JSON：游戏自己写出的 profile 是单行，
    // 用 pretty 会把体积近乎翻倍（实测 452KB → 980KB）且与游戏格式不一致。
    // 导出给用户看的文件（inspect/presets 的导出）与编辑器自己的配置仍用 pretty。
    let out = serde_json::to_string(&raw).map_err(|e| e.to_string())?;
    fs::write(&path, out).map_err(|e| e.to_string())?;
    Ok(SaveResult {
        backup_path: backup_path.map(|p| p.to_string_lossy().to_string()),
    })
}

/// 「准备转生」的结果
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrestigePrep {
    /// 写入的待执行档位（`spt.pendingPrestige.prestigeLevel`）
    pub level: i64,
    /// 写入前的备份路径
    pub backup_path: Option<String>,
}

/// 读 PMC 当前转生等级（`Info.PrestigeLevel`）
fn current_prestige(raw: &Value) -> i64 {
    raw.pointer("/characters/pmc/Info/PrestigeLevel")
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
}

/// 纯逻辑：把「准备转生」写进内存里的存档对象。抽出来是为了不依赖 AppHandle 也能测。
pub(crate) fn apply_pending(raw: &mut Value, level: i64, max: i64) -> Result<i64, String> {
    if max <= 0 {
        return Err("读不到转生档位表（templates/prestige.json），已停止，未改动存档".to_string());
    }
    // 服务器算法：`num = clamp(PrestigeLevel - 1, 0, max - 1)`，所以只能准备「当前等级 + 1」
    let current = current_prestige(raw);
    let expected = current + 1;
    if level != expected {
        return Err(format!(
            "只能准备下一档：当前 {} 级，目标必须是 {} 级",
            current, expected
        ));
    }
    if expected > max {
        return Err(format!("已经到顶：上限 {} 档", max));
    }
    let spt = raw
        .get_mut("spt")
        .ok_or_else(|| "存档缺少 spt 段".to_string())?;
    let obj = spt
        .as_object_mut()
        .ok_or_else(|| "存档的 spt 段不是对象".to_string())?;
    // items 留空：玩家不指定转移清单时不带走任何仓库物品（服务器据此只发档位奖励）
    obj.insert(
        "pendingPrestige".to_string(),
        json!({ "prestigeLevel": level, "items": [] }),
    );
    Ok(level)
}

/// 「准备转生」：只写 `spt.pendingPrestige`，把「清等级 / 清任务 / 清仓库 / 清藏身处 / 转移技能 /
/// 发奖励 / 补成就 / 写 Prestige 记录」交给服务器在玩家「重置角色」时执行。
///
/// 为什么不在编辑器里直接复刻：真正的转生发生在 `CreateProfileService.CreateProfile`（372 行，
/// 依赖 20+ 个服务与数据库），编辑器只读存档文件，复刻必然走样。写标记是唯一可靠且不改坏存档的路。
pub fn prestige_prepare(app: &AppHandle, id: &str, level: i64) -> Result<PrestigePrep, String> {
    if !is_safe_id(id) {
        return Err(format!("Invalid profile id: {}", id));
    }
    let dir = profiles_dir(app).ok_or_else(|| "未选择 SPT 服务器".to_string())?;
    let path = Path::new(&dir).join(format!("{}.json", id));
    let txt = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut raw: Value = serde_json::from_str(&txt).map_err(|e| e.to_string())?;

    let max = prestige_max(app);
    apply_pending(&mut raw, level, max)?;

    let backup_path = backup_before_write(app, &path)?;
    let out = serde_json::to_string(&raw).map_err(|e| e.to_string())?;
    fs::write(&path, out).map_err(|e| e.to_string())?;
    Ok(PrestigePrep {
        level,
        backup_path: backup_path.map(|p| p.to_string_lossy().to_string()),
    })
}

/// 取消已准备的转生：删掉 `spt.pendingPrestige`，其余一律不动
pub fn prestige_cancel(app: &AppHandle, id: &str) -> Result<Option<String>, String> {
    if !is_safe_id(id) {
        return Err(format!("Invalid profile id: {}", id));
    }
    let dir = profiles_dir(app).ok_or_else(|| "未选择 SPT 服务器".to_string())?;
    let path = Path::new(&dir).join(format!("{}.json", id));
    let txt = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut raw: Value = serde_json::from_str(&txt).map_err(|e| e.to_string())?;
    let Some(obj) = raw.get_mut("spt").and_then(|v| v.as_object_mut()) else {
        return Ok(None);
    };
    if obj.remove("pendingPrestige").is_none() {
        return Ok(None);
    }
    let backup_path = backup_before_write(app, &path)?;
    let out = serde_json::to_string(&raw).map_err(|e| e.to_string())?;
    fs::write(&path, out).map_err(|e| e.to_string())?;
    Ok(backup_path.map(|p| p.to_string_lossy().to_string()))
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

    // ———————— 转生 ————————

    /// 上限取 `elements` 条数：4.1.6 的 `templates/prestige.json` 是 `{elements:[…]}`
    #[test]
    fn prestige_max_counts_elements() {
        assert_eq!(parse_prestige_max(r#"{"elements":[{},{},{},{}]}"#), 4);
        assert_eq!(parse_prestige_max(r#"{"elements":[]}"#), 0);
        assert_eq!(parse_prestige_max(r#"{"elements":{}}"#), 0);
        assert_eq!(parse_prestige_max("not json"), 0);
        assert_eq!(parse_prestige_max("{}"), 0);
    }

    /// 读不到上限时只挡负数，绝不把玩家的转生等级压成 0
    #[test]
    fn prestige_clamp_never_silently_zeroes() {
        assert_eq!(clamp_prestige(9, 4), 4);
        assert_eq!(clamp_prestige(2, 4), 2);
        assert_eq!(clamp_prestige(-3, 4), 0);
        assert_eq!(clamp_prestige(3, 0), 3, "上限未知时必须保留原值");
        assert_eq!(clamp_prestige(-1, 0), 0);
    }

    /// PMC 写 Info.PrestigeLevel；Scav 侧不写（Scav 没有转生）
    #[test]
    fn prestige_written_only_for_pmc() {
        let mk = |is_pmc: bool| {
            let mut side = json!({ "Info": { "Level": 10 } });
            let data = CharSideData {
                level: 10,
                prestige: 2,
                ..Default::default()
            };
            write_side(Some(&mut side), &data, is_pmc);
            side
        };
        assert_eq!(mk(true).pointer("/Info/PrestigeLevel"), Some(&json!(2)));
        assert!(mk(false).pointer("/Info/PrestigeLevel").is_none());
    }

    /// 本机真实库自检（默认忽略）：4.1.6 转生档位是 4 个
    #[test]
    #[ignore = "需要本机 SPT 安装 E:\\EFT"]
    fn real_prestige_template_has_four_tiers() {
        let txt = fs::read_to_string(r"E:\EFT\SPT_Runtime\SPT_Data\database\templates\prestige.json")
            .expect("读 prestige.json 失败");
        assert_eq!(parse_prestige_max(&txt), 4, "转生档位数不对");
        let v: Value = serde_json::from_str(&txt).expect("解析失败");
        let ids: Vec<&str> = v["elements"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e.get("id").and_then(|x| x.as_str()).unwrap_or(""))
            .collect();
        assert_eq!(ids[0], "672df12f97f0469cea52f55e");
        assert!(ids.iter().all(|s| s.len() == 24), "转生 id 不是 24 位");
    }

    /// 递归比对两份 JSON，只允许 `/characters/pmc/Info/PrestigeLevel` 这一条路径不同。
    /// 返回实际发生差异的路径列表，用于断言「除转生等级外逐字不变」。
    fn diff_paths(a: &Value, b: &Value, base: &str, out: &mut Vec<String>) {
        match (a, b) {
            (Value::Object(ma), Value::Object(mb)) => {
                for (k, va) in ma {
                    let path = format!("{}/{}", base, k);
                    match mb.get(k) {
                        Some(vb) => diff_paths(va, vb, &path, out),
                        None => out.push(format!("{} (仅左侧存在)", path)),
                    }
                }
                for k in mb.keys() {
                    if !ma.contains_key(k) {
                        out.push(format!("{}/{} (仅右侧存在)", base, k));
                    }
                }
            }
            (Value::Array(xa), Value::Array(xb)) => {
                if xa.len() != xb.len() {
                    out.push(format!("{} 长度 {}→{}", base, xa.len(), xb.len()));
                    return;
                }
                for (i, (va, vb)) in xa.iter().zip(xb.iter()).enumerate() {
                    diff_paths(va, vb, &format!("{}/{}", base, i), out);
                }
            }
            _ => {
                if a != b {
                    out.push(base.to_string());
                }
            }
        }
    }

    /// 本机真机自检（默认忽略）：只把转生等级从 0 改成 1，存档里除该字段外必须逐字不变。
    /// 读 `E:\EFT` 的探针档（不存在则取任一真实档，只在内存里比对，不写盘）。
    #[test]
    #[ignore = "需要本机 SPT 存档 E:\\EFT"]
    fn real_profile_prestige_write_touches_only_that_field() {
        let dir = r"E:\EFT\SPT_Runtime\user\profiles";
        let probe = Path::new(dir).join("0000000000000000000000a1.json");
        let path = if probe.exists() {
            probe
        } else {
            let mut picked = None;
            for e in fs::read_dir(dir).expect("读 profiles 目录失败") {
                let p = e.expect("遍历失败").path();
                if p.extension().and_then(|x| x.to_str()) == Some("json") {
                    picked = Some(p);
                    break;
                }
            }
            picked.expect("目录里没有 json")
        };
        let txt = fs::read_to_string(&path).expect("读存档失败");
        let mut raw: Value = serde_json::from_str(&txt).expect("解析存档失败");
        let before = raw.clone();

        // 用档案自身的真实数据回填，只改转生等级 —— 这样才能断言「其它字段逐字不变」
        let old = before
            .pointer("/characters/pmc/Info/PrestigeLevel")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let mut data = read_side(before.pointer("/characters/pmc"));
        // 目标值与现值不同，保证确实发生了写入
        data.prestige = if old == 0 { 1 } else { 0 };
        write_side(raw.pointer_mut("/characters/pmc"), &data, true);

        let mut diffs = Vec::new();
        diff_paths(&before, &raw, "", &mut diffs);
        // 允许的差异：PrestigeLevel（本次目标）、Level/Experience（write_side 恒写，值相同则不算差异）、
        // Nickname/LowerNickname/Side（data 为空串时 write_side 仍会写 nickname，这里只允许值相同的写入）
        let unexpected: Vec<&String> = diffs
            .iter()
            .filter(|p| !p.ends_with("/Info/PrestigeLevel"))
            .collect();
        assert!(
            unexpected.is_empty(),
            "除转生等级外还有字段被改动: {:?}",
            unexpected
        );
        assert_eq!(
            raw.pointer("/characters/pmc/Info/PrestigeLevel"),
            Some(&json!(data.prestige)),
            "写入后的转生等级不对"
        );
        assert_eq!(
            before.pointer("/characters/pmc/Info/PrestigeLevel"),
            Some(&json!(old)),
            "前置条件：原件转生等级应保持未变"
        );
    }

    // —— 「准备转生」：只写 spt.pendingPrestige ——

    #[test]
    fn pending_refuses_when_tier_table_missing() {
        let mut raw = json!({ "spt": {} });
        assert!(apply_pending(&mut raw, 1, 0).is_err());
        // 失败时不得留下任何写入
        assert!(raw.pointer("/spt/pendingPrestige").is_none());
    }

    #[test]
    fn pending_only_accepts_the_next_tier() {
        let mut raw = json!({
            "spt": {},
            "characters": { "pmc": { "Info": { "PrestigeLevel": 1 } } }
        });
        // 当前 1 级 → 只能准备 2 级
        assert!(apply_pending(&mut raw, 1, 4).is_err());
        assert!(apply_pending(&mut raw, 3, 4).is_err());
        assert!(apply_pending(&mut raw, 2, 4).is_ok());
        assert_eq!(
            raw.pointer("/spt/pendingPrestige/prestigeLevel"),
            Some(&json!(2))
        );
        // 转移清单留空（不指定就不带走仓库物品）
        assert_eq!(raw.pointer("/spt/pendingPrestige/items"), Some(&json!([])));
    }

    #[test]
    fn pending_at_ceiling_is_refused() {
        let mut raw = json!({
            "spt": {},
            "characters": { "pmc": { "Info": { "PrestigeLevel": 4 } } }
        });
        let err = apply_pending(&mut raw, 5, 4).expect_err("到顶后必须拒绝");
        assert!(err.contains("已经到顶"), "错误信息不含预期内容: {}", err);
        assert!(raw.pointer("/spt/pendingPrestige").is_none());
    }

    #[test]
    fn pending_leaves_everything_else_untouched() {
        let mut raw = json!({
            "spt": { "version": "4.1.6 (40743)", "mods": [] },
            "characters": { "pmc": { "Info": { "PrestigeLevel": 0, "Level": 42 } } }
        });
        let before = raw.clone();
        apply_pending(&mut raw, 1, 4).expect("准备失败");
        let mut diffs = Vec::new();
        diff_paths(&before, &raw, "", &mut diffs);
        let unexpected: Vec<&String> = diffs
            .iter()
            .filter(|p| !p.starts_with("/spt/pendingPrestige"))
            .collect();
        assert!(unexpected.is_empty(), "其它字段被改动: {:?}", unexpected);
    }

    /// 真机：真实档走一遍「准备转生」，除 `spt.pendingPrestige` 外逐字不变（只在内存里比，不写盘）
    #[test]
    #[ignore = "需要本机 SPT 安装 E:\\EFT"]
    fn real_profile_pending_prepare_touches_only_that_key() {
        let dir = r"E:\EFT\SPT_Runtime\user\profiles";
        // 只挑「还没有待执行转生」的档：已经准备过的档再准备一次会被规则拒绝，不是缺陷
        let picked = fs::read_dir(dir)
            .expect("读 profiles 目录失败")
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
            .find_map(|p| {
                let txt = fs::read_to_string(&p).ok()?;
                let v: Value = serde_json::from_str(&txt).ok()?;
                if v.pointer("/spt/pendingPrestige").is_some() {
                    return None;
                }
                Some((p, txt, v))
            });
        let Some((_path, _txt, before)) = picked else {
            return; // 全都已准备过转生，跳过
        };
        let mut raw = before.clone();

        let old = current_prestige(&before);
        let max = parse_prestige_max(
            &fs::read_to_string(r"E:\EFT\SPT_Runtime\SPT_Data\database\templates\prestige.json")
                .expect("读 prestige.json 失败"),
        );
        // 已经到顶的档没法再准备，跳过（不是失败）
        if old + 1 > max {
            return;
        }
        apply_pending(&mut raw, old + 1, max).expect("准备转生失败");

        let mut diffs = Vec::new();
        diff_paths(&before, &raw, "", &mut diffs);
        let unexpected: Vec<&String> = diffs
            .iter()
            .filter(|p| !p.starts_with("/spt/pendingPrestige"))
            .collect();
        assert!(unexpected.is_empty(), "其它字段被改动: {:?}", unexpected);
        assert_eq!(
            raw.pointer("/spt/pendingPrestige/prestigeLevel"),
            Some(&json!(old + 1))
        );
        assert!(
            before.pointer("/spt/pendingPrestige").is_none(),
            "前置条件：原件不应本来就有 pendingPrestige"
        );
    }
}
