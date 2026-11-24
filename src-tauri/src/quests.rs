//! 任务页（05-quests）数据服务 —— 读全量 / 写回。
//!
//! 数据口径（对照参考编辑器 CharacterQuest.cs / Character.cs / AppSettings.cs）：
//! - 档案 /characters/pmc/Quests：条目 camelCase（qid/startTime/status/statusTimers），
//!   status 由服务器序列化为**整数**（4.1.6 真实档 `"status": 4`，即 Locked=0 … AvailableAfter=9）；
//!   内部统一用枚举名处理，读写边界由 status_of / status_value 转换（旧档的枚举名字符串仍可读）；
//! - 任务类型推断（对齐 Character.UpdateQuestsData）：locale 含 "{qid} name" 或无
//!   重复任务组 → Standart（trader 取 templates，查无 "unknown"）；否则扫
//!   RepeatableQuests 各组命中 _id → 组名 + 条目 traderId，显示名取 locale
//!   "DailyQuestName/{条目type}" 回退条目 type 字符串；
//! - 状态合法性（对齐 AppSettings.GetAvailableStatuses）：常规 6 值、重复 5 值
//!   （无 Locked）；非法写入按 GetNewStatus 就近修正（清单序取 First），
//!   无匹配保持原状态；
//! - 状态时间轴 statusTimers：键 = 状态整数（真实档 `{"1":…,"4":…}`），值 Unix 秒，只增不更；
//! - 完成联动（对齐 SetupCraftForQuest）：任务 → Success 时把联动配方
//!   （locked 且 requirements 含 QuestComplete=本任务）加入
//!   UnlockedInfo.unlockedProductionRecipe，离开 Success 移除；只动变更任务
//!   联动的配方，绝不整体重算；
//! - 缺失清单 missing = templates − 档案 − 事件任务（configs/quest.json
//!   eventQuests），供「补齐缺失」以锁定状态补入。
//! 写回铁律：先备份，后定点替换；差分语义：已有改状态 / 缺失移除 / 新增补入。

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tauri::AppHandle;

use crate::backup::backup_before_write;
use crate::character::database_dir;
use crate::profile::{is_safe_id, profiles_dir, SaveResult};

// ———————— 类型 ————————

/// 任务行：档案条目原样字段 + 类型推断 + 解析显示名
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestRow {
    pub qid: String,
    /// 任务类型：Standart / Weekly / Daily / Daily_Savage
    #[serde(rename = "type")]
    pub qtype: String,
    /// 商人 id（Standart 取 templates，查无 "unknown"；重复任务取条目 traderId）
    pub trader: String,
    pub status: String,
    pub start_time: f64,
    /// 状态时间轴原样对象（键 = 状态枚举名，值 = Unix 秒）
    pub status_timers: Value,
    /// 解析后的显示名（locale）；空串由前端回退
    pub quest_name: String,
    /// 完成联动提示（已解锁配方产物名；仅 Success 且配方已解锁时给出）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unlock_hint: Option<String>,
    /// 完成条件总数（templates 的 conditions.AvailableForFinish；三桶形态已合并）
    pub cond_total: i64,
    /// 条件里已勾掉的条数（档案 completedConditions ∩ 条件 id 集）
    pub cond_done: i64,
  /// 逐条条件是否完成（与 templates 条件顺序一致；条件超过 40 条时为空 = 只给汇总）
  pub cond_flags: Vec<bool>,
    /// 奖励摘要：经验 / 商人好感 / 卢布（其余奖励类型不进摘要，不拼凑）
    pub reward_exp: i64,
    pub reward_standing: f64,
    pub reward_money: i64,
}

/// 任务页全量数据
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestsData {
    pub rows: Vec<QuestRow>,
    /// 缺失任务完整行（templates − 档案 − 事件任务），供前端以锁定状态补入
    pub missing: Vec<QuestRow>,
}

/// 任务写回行（全量差分：qid + 目标状态）
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestSaveRow {
    pub qid: String,
    pub status: String,
}

/// 任务页写回载荷
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestsSaveData {
    pub rows: Vec<QuestSaveRow>,
}

// ———————— 状态语义（CharacterQuest.cs） ————————

/// QuestStatus 数值（声明序，即 GetNewStatus 的比较基准）
fn status_num(status: &str) -> i64 {
    match status {
        "Locked" => 0,
        "AvailableForStart" => 1,
        "Started" => 2,
        "AvailableForFinish" => 3,
        "Success" => 4,
        "Fail" => 5,
        "FailRestartable" => 6,
        "MarkedAsFailed" => 7,
        "Expired" => 8,
        "AvailableAfter" => 9,
        _ => -1,
    }
}

/// QuestStatus 数值 → 枚举名（服务器序列化方向：档案里 status 存整数，见 4.1.6 真实档）
fn status_name(n: i64) -> &'static str {
    match n {
        0 => "Locked",
        1 => "AvailableForStart",
        2 => "Started",
        3 => "AvailableForFinish",
        4 => "Success",
        5 => "Fail",
        6 => "FailRestartable",
        7 => "MarkedAsFailed",
        8 => "Expired",
        9 => "AvailableAfter",
        _ => "Locked",
    }
}

/// 读档案里的 status：4.1 起为整数（`4`），旧档可能是枚举名字符串；统一归到枚举名供内部使用。
/// 未知数值按 Locked 处理（服务器侧也只承认 0..9）
fn status_of(entry: &Value) -> String {
    match entry.get("status") {
        Some(Value::Number(n)) => status_name(n.as_i64().unwrap_or(0)).to_string(),
        Some(Value::String(s)) => s.clone(),
        _ => "Locked".to_string(),
    }
}

/// 写档案时 status 一律落整数（对齐服务器序列化）：`4` 而不是 `"Success"`
fn status_value(status: &str) -> Value {
    Value::from(status_num(status).max(0))
}

/// statusTimers 的键同样是状态整数（真实档 `{"1":…, "4":…}`）
fn timer_key(status: &str) -> String {
    status_num(status).max(0).to_string()
}

/// 合法状态清单（清单序 = GetNewStatus 取 First 的优先序；AppSettings.cs）：
/// 常规 6 值；重复任务（Weekly/Daily/Daily_Savage）无 Locked 态
fn legal_statuses(quest_type: &str) -> &'static [&'static str] {
    if quest_type == "Standart" {
        &[
            "Locked",
            "AvailableForStart",
            "Started",
            "Fail",
            "AvailableForFinish",
            "Success",
        ]
    } else {
        &[
            "AvailableForStart",
            "Started",
            "Fail",
            "AvailableForFinish",
            "Success",
        ]
    }
}

/// 对齐 CharacterQuest.Status setter：合法直写；非法按 GetNewStatus 就近修正
/// （目标比当前小取比目标大的清单序 First，反之取比目标小的 First），
/// 无匹配保持原状态
fn resolve_status(quest_type: &str, current: &str, requested: &str) -> String {
    let legal = legal_statuses(quest_type);
    if legal.contains(&requested) {
        return requested.to_string();
    }
    let req_num = status_num(requested);
    let cur_num = status_num(current);
    let picked = legal.iter().find(|s| {
        let n = status_num(s);
        if req_num < cur_num { n > req_num } else { n < req_num }
    });
    picked.map(|s| s.to_string()).unwrap_or_else(|| current.to_string())
}

// ———————— 任务上下文（服务器数据目录一次读齐） ————————

#[derive(Default)]
struct QuestCtx {
    /// locale 键值对（任务名 / 物品名 / 重复任务类型显示名）
    locale: HashMap<String, String>,
    /// templates/quests.json：qid → traderId
    template_traders: HashMap<String, String>,
    /// templates 全集（算缺失用，保持文件顺序）
    template_ids: Vec<String>,
    /// configs/quest.json eventQuests 键集（事件任务不进缺失清单）
    event_quests: HashSet<String>,
    /// 联动：qid → 配方 id 列表（locked:true 且 requirements 含 QuestComplete）
    craft_by_quest: HashMap<String, Vec<String>>,
    /// 配方 id → endProduct 模板 id
    craft_end_product: HashMap<String, String>,
    /// 档案 RepeatableQuests：组名 → [(条目_id, 条目traderId, 条目type)]
    repeatable: Vec<(String, Vec<(String, String, String)>)>,
    /// qid → 完成条件 id 集（templates conditions，三桶形态已合并）
    quest_conds: HashMap<String, Vec<String>>,
    /// qid → 奖励摘要（经验, 好感, 卢布）
    quest_rewards: HashMap<String, (i64, f64, i64)>,
}

/// 把一桶条件数组里的 id 收进 out（数组或对象形态都吃）
fn push_cond_ids(arr: &Value, out: &mut Vec<String>) {
    if let Some(a) = arr.as_array() {
        for c in a {
            if let Some(id) = c.get("id").and_then(|x| x.as_str()) {
                out.push(id.to_string());
            }
        }
    }
}

/// 任务完成条件 id 集：4.1 是 `{AvailableForStart,AvailableForFinish,Fail}` 三桶，旧版是数组
fn cond_ids_of(quest: &Value) -> Vec<String> {
    let mut out = Vec::new();
    match quest.pointer("/conditions") {
        Some(Value::Array(_)) => push_cond_ids(&quest.pointer("/conditions").unwrap(), &mut out),
        Some(Value::Object(m)) => {
            for key in ["AvailableForStart", "AvailableForFinish", "Fail"] {
                if let Some(bucket) = m.get(key) {
                    push_cond_ids(bucket, &mut out);
                }
            }
        }
        _ => {}
    }
    out
}

/// 奖励摘要：只取经验 / 商人好感 / 卢布三种可确定数值的，别的类型不臆测
fn reward_summary(quest: &Value) -> (i64, f64, i64) {
    let mut exp = 0i64;
    let mut standing = 0.0f64;
    let mut money = 0i64;
    // 4.1 的 rewards 是三桶对象 { Fail, Started, Success }，只取 Success；
    // 旧版是裸数组，两种都认。
    let success = match quest.get("rewards") {
        Some(Value::Array(_)) => quest.get("rewards"),
        Some(Value::Object(m)) => m.get("Success"),
        _ => None,
    };
    if let Some(arr) = success.and_then(|v| v.as_array()) {
        for r in arr {
            match r.get("type").and_then(|v| v.as_str()).unwrap_or("") {
                "Experience" => exp += r.get("value").and_then(|v| v.as_i64()).unwrap_or(0),
                "TraderStanding" => {
                    standing += r.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0)
                }
                "Item" => {
                    if let Some(items) = r.get("items").and_then(|v| v.as_array()) {
                        for it in items {
                            // 只统计卢布现金；其他物品奖励没有统一「数量」口径，不累加
                            if it.get("_tpl").and_then(|v| v.as_str()) == Some("5449016a4bdc2d6f028b456f")
                            {
                                money += it
                                    .pointer("/upd/StackObjectsCount")
                                    .and_then(|v| v.as_i64())
                                    .unwrap_or(1);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    (exp, standing, money)
}

/// 条件进度：(总数, 已完成数, 逐条完成标记)。archive 为空表示缺失任务（没进档，按 0 完成）
/// flags 与 templates 里的条件顺序一一对应，前端用它画逐条打勾
fn cond_progress(qid: &str, archive: Option<&Value>, ctx: &QuestCtx) -> (i64, i64, Vec<bool>) {
    let Some(ids) = ctx.quest_conds.get(qid) else {
        return (0, 0, Vec::new());
    };
    let total = ids.len() as i64;
    let Some(entry) = archive else {
        return (total, 0, vec![false; ids.len()]);
    };
    // 档案 completedConditions 是字符串数组；老档偶见 {id} 对象，两种都认
    let mut done = 0i64;
    let mut finished: Vec<&str> = Vec::new();
    if let Some(arr) = entry.get("completedConditions").and_then(|v| v.as_array()) {
        for c in arr {
            let cid = c
                .as_str()
                .or_else(|| c.get("id").and_then(|v| v.as_str()))
                .unwrap_or("");
            if ids.iter().any(|x| x == cid) {
                done += 1;
                finished.push(cid);
            }
        }
    }
    let flags = if ids.len() > 40 {
        // 条件特别多的任务只给汇总，避免一行圆点刷屏
        Vec::new()
    } else {
        ids.iter().map(|id| finished.contains(&id.as_str())).collect()
    };
    (total, done, flags)
}

/// 从服务器数据目录读齐任务页上下文；locale 按 language 回退 en.json
fn load_ctx(app: &AppHandle, language: &str) -> QuestCtx {
    let mut ctx = QuestCtx::default();
    let Some(dir) = database_dir(app) else {
        return ctx;
    };
    let db = Path::new(&dir);

    // templates/quests.json：4.1 起为字典 { qid -> quest }（键即 qid，值含 _id/traderId），
    // 更早版本为数组；两种形态都收
    if let Ok(txt) = fs::read_to_string(db.join("templates").join("quests.json")) {
        if let Ok(parsed) = serde_json::from_str::<Value>(&txt) {
            // 保留整条模板：除了 traderId，还要拿 conditions 与 rewards 供右栏详情用
            let entries: Vec<(String, Value)> = match &parsed {
                Value::Object(map) => map
                    .iter()
                    .map(|(key, t)| {
                        let qid = t
                            .get("_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or(key.as_str())
                            .to_string();
                        (qid, t.clone())
                    })
                    .collect(),
                Value::Array(arr) => arr
                    .iter()
                    .filter_map(|t| {
                        let qid = t.get("_id").and_then(|v| v.as_str())?.to_string();
                        Some((qid, t.clone()))
                    })
                    .collect(),
                _ => Vec::new(),
            };
            for (qid, t) in entries {
                let trader = t
                    .get("traderId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                ctx.quest_conds.insert(qid.clone(), cond_ids_of(&t));
                ctx.quest_rewards.insert(qid.clone(), reward_summary(&t));
                ctx.template_traders.insert(qid.clone(), trader);
                ctx.template_ids.push(qid);
            }
        }
    }

    // configs/quest.json：eventQuests 键集。
    // 位置在 SPT_Data/configs（不在 database 下），4.1.6 实测 30 个配置文件同处
    if let Some(data_root) = crate::character::spt_data_dir(app) {
        let cfg_path = Path::new(&data_root).join("configs").join("quest.json");
        if let Ok(txt) = fs::read_to_string(cfg_path) {
            if let Ok(cfg) = serde_json::from_str::<Value>(&txt) {
                if let Some(map) = cfg.get("eventQuests").and_then(|v| v.as_object()) {
                    ctx.event_quests.extend(map.keys().cloned());
                }
            }
        }
    }

    // hideout/production.json：locked 配方的任务联动与产物。
    // 顶层形态为 { recipes: [...] }（参考 AppData.LoadHideoutProduction 的 HideoutProductions.Recipes）
    if let Ok(txt) = fs::read_to_string(db.join("hideout").join("production.json")) {
        let parsed: Value = match serde_json::from_str(&txt) {
            Ok(v) => v,
            Err(_) => Value::Null,
        };
        let recipes = parsed
            .get("recipes")
            .and_then(|r| r.as_array())
            .or_else(|| parsed.as_array());
        if let Some(recipes) = recipes {
            for p in recipes {
                let Some(rid) = p.get("_id").and_then(|v| v.as_str()) else {
                    continue;
                };
                if let Some(ep) = p.get("endProduct").and_then(|v| v.as_str()) {
                    ctx.craft_end_product.insert(rid.to_string(), ep.to_string());
                }
                if !p.get("locked").and_then(|v| v.as_bool()).unwrap_or(false) {
                    continue;
                }
                let Some(reqs) = p.get("requirements").and_then(|v| v.as_array()) else {
                    continue;
                };
                for req in reqs {
                    if req.get("type").and_then(|v| v.as_str()) == Some("QuestComplete") {
                        if let Some(qid) = req.get("questId").and_then(|v| v.as_str()) {
                            ctx.craft_by_quest
                                .entry(qid.to_string())
                                .or_default()
                                .push(rid.to_string());
                        }
                    }
                }
            }
        }
    }

    // locales/global/{language}.json，缺失回退 en.json
    for lang in [language, "en"] {
        let path = db.join("locales").join("global").join(format!("{}.json", lang));
        let Ok(Value::Object(map)) =
            serde_json::from_str::<Value>(&fs::read_to_string(path).unwrap_or_default())
        else {
            continue;
        };
        for (k, v) in map {
            if let Some(s) = v.as_str() {
                ctx.locale.insert(k, s.to_string());
            }
        }
        break;
    }

    ctx
}

/// 档案 RepeatableQuests → 组清单（activeQuests 在前、inactiveQuests 在后）
fn repeatable_groups(pmc: &Value) -> Vec<(String, Vec<(String, String, String)>)> {
    let mut groups = Vec::new();
    let Some(arr) = pmc.get("RepeatableQuests").and_then(|v| v.as_array()) else {
        return groups;
    };
    for g in arr {
        let Some(name) = g.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        let mut items = Vec::new();
        for key in ["activeQuests", "inactiveQuests"] {
            let Some(qs) = g.get(key).and_then(|v| v.as_array()) else {
                continue;
            };
            for q in qs {
                let Some(qid) = q.get("_id").and_then(|v| v.as_str()) else {
                    continue;
                };
                items.push((
                    qid.to_string(),
                    q.get("traderId")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    q.get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown")
                        .to_string(),
                ));
            }
        }
        groups.push((name.to_string(), items));
    }
    groups
}

/// 对齐 Character.UpdateQuestsData：推断任务类型 + 商人 + 显示名
fn resolve_row_info(qid: &str, ctx: &QuestCtx) -> (String, String, String) {
    let standart = || {
        let trader = ctx
            .template_traders
            .get(qid)
            .filter(|s| !s.is_empty())
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        let name = ctx
            .locale
            .get(&format!("{} name", qid))
            .cloned()
            .unwrap_or_default();
        ("Standart".to_string(), trader, name)
    };
    if ctx.locale.contains_key(&format!("{} name", qid)) || ctx.repeatable.is_empty() {
        return standart();
    }
    for (group, items) in &ctx.repeatable {
        if let Some((_, tid, ttype)) = items.iter().find(|(id, _, _)| id == qid) {
            let name = ctx
                .locale
                .get(&format!("DailyQuestName/{}", ttype))
                .cloned()
                .unwrap_or_else(|| ttype.clone());
            return (group.clone(), tid.clone(), name);
        }
    }
    standart()
}

/// 完成联动提示：Success 且联动的 locked 配方已解锁 → 产物 locale 名
fn unlock_hint(
    qid: &str,
    status: &str,
    ctx: &QuestCtx,
    unlocked: &HashSet<String>,
) -> Option<String> {
    if status != "Success" {
        return None;
    }
    let recipes = ctx.craft_by_quest.get(qid)?;
    let rid = recipes.iter().find(|r| unlocked.contains(r.as_str()))?;
    let tpl = ctx.craft_end_product.get(rid)?;
    ctx.locale.get(&format!("{} Name", tpl)).cloned()
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ———————— 读 ————————

/// 任务页全量数据；档案缺失或不可读时为 None
pub fn load_quests(app: &AppHandle, id: &str) -> Option<QuestsData> {
    if !is_safe_id(id) {
        return None;
    }
    let dir = profiles_dir(app)?;
    let raw: Value =
        serde_json::from_str(&fs::read_to_string(Path::new(&dir).join(format!("{}.json", id))).ok()?)
            .ok()?;
    let pmc = raw.pointer("/characters/pmc")?;
    let language = crate::server::data_language(app, Some(&raw));
    let mut ctx = load_ctx(app, &language);
    ctx.repeatable = repeatable_groups(pmc);

    let unlocked: HashSet<String> = pmc
        .pointer("/UnlockedInfo/unlockedProductionRecipe")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let mut rows = Vec::new();
    if let Some(arr) = pmc.get("Quests").and_then(|q| q.as_array()) {
        for entry in arr {
            let Some(qid) = entry.get("qid").and_then(|v| v.as_str()) else {
                continue;
            };
            let status = status_of(entry);
            let (qtype, trader, quest_name) = resolve_row_info(qid, &ctx);
            let (cond_total, cond_done, cond_flags) = cond_progress(qid, Some(entry), &ctx);
            let (reward_exp, reward_standing, reward_money) = ctx
                .quest_rewards
                .get(qid)
                .copied()
                .unwrap_or((0, 0.0, 0));
            rows.push(QuestRow {
                qid: qid.to_string(),
                qtype,
                trader,
                status: status.clone(),
                start_time: entry.get("startTime").and_then(|v| v.as_f64()).unwrap_or(0.0),
                status_timers: entry
                    .get("statusTimers")
                    .cloned()
                    .unwrap_or_else(|| Value::Object(Map::new())),
                quest_name,
                unlock_hint: unlock_hint(qid, &status, &ctx, &unlocked),
                cond_total,
                cond_done,
                cond_flags,
                reward_exp,
                reward_standing,
                reward_money,
            });
        }
    }

    // 缺失清单：templates − 档案 − 事件任务；带判型与显示名，供前端以锁定状态补入
    let present: HashSet<&str> = rows.iter().map(|r| r.qid.as_str()).collect();
    let missing_ids: Vec<String> = ctx
        .template_ids
        .iter()
        .filter(|qid| {
            !present.contains(qid.as_str()) && !ctx.event_quests.contains(qid.as_str())
        })
        .cloned()
        .collect();
    let missing: Vec<QuestRow> = missing_ids
        .iter()
        .map(|qid| {
            let (qtype, trader, quest_name) = resolve_row_info(qid, &ctx);
            let (cond_total, cond_done, cond_flags) = cond_progress(qid, None, &ctx);
            let (reward_exp, reward_standing, reward_money) = ctx
                .quest_rewards
                .get(qid)
                .copied()
                .unwrap_or((0, 0.0, 0));
            QuestRow {
                qid: qid.clone(),
                qtype,
                trader,
                status: "Locked".to_string(),
                start_time: 0.0,
                status_timers: Value::Object(Map::new()),
                quest_name,
                unlock_hint: None,
                cond_total,
                cond_done,
                cond_flags,
                reward_exp,
                reward_standing,
                reward_money,
            }
        })
        .collect();

    Some(QuestsData { rows, missing })
}

/// 任务页写回：先备份，后差分定点替换。
///
/// 差分语义（对齐参考编辑器）：
/// - 前端发全量 `[{qid, status}]`；档案已有条目 → 按 setter 语义改状态
///   （合法直写 / 非法就近修正 + statusTimers 只增不更 + 联动配方增量）；
/// - 档案有、前端没有 → 移除条目（不动联动配方）；
/// - 前端有、档案没有 → 以 `{qid, startTime: 0, status, statusTimers}` 补入
///   （timers 恒含 Locked；目标状态非 Locked 再记该状态时间）。
pub fn save_quests(app: &AppHandle, id: &str, data: &QuestsSaveData) -> Result<SaveResult, String> {
    if !is_safe_id(id) {
        return Err(format!("Invalid profile id: {}", id));
    }
    let dir = profiles_dir(app).ok_or_else(|| "未选择 SPT 服务器".to_string())?;
    let path = Path::new(&dir).join(format!("{}.json", id));
    let txt = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut raw: Value = serde_json::from_str(&txt).map_err(|e| e.to_string())?;
    let backup_path = backup_before_write(app, &path)?;

    let language = crate::server::data_language(app, Some(&raw));
    let mut ctx = load_ctx(app, &language);
    if let Some(pmc) = raw.pointer("/characters/pmc") {
        ctx.repeatable = repeatable_groups(pmc);
    }

    let wanted: HashMap<&str, &str> = data
        .rows
        .iter()
        .map(|r| (r.qid.as_str(), r.status.as_str()))
        .collect();
    let now = unix_now();
    let mut unlock_add: Vec<String> = Vec::new();
    let mut unlock_remove: Vec<String> = Vec::new();
    let mut new_entries: Vec<Value> = Vec::new();

    // Quests 数组缺失则补空数组
    if raw.pointer("/characters/pmc/Quests").is_none() {
        if let Some(pmc) = raw.pointer_mut("/characters/pmc") {
            if let Some(obj) = pmc.as_object_mut() {
                obj.insert("Quests".to_string(), Value::Array(Vec::new()));
            }
        }
    }

    if let Some(quests) = raw
        .pointer_mut("/characters/pmc/Quests")
        .and_then(|v| v.as_array_mut())
    {
        // 一轮：修改已有条目（setter 语义 + 联动增量），收集待删
        let mut remove: HashSet<String> = HashSet::new();
        for entry in quests.iter_mut() {
            let Some(qid) = entry
                .get("qid")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
            else {
                continue;
            };
            match wanted.get(qid.as_str()) {
                None => {
                    remove.insert(qid);
                }
                Some(&new_status) => {
                    let (qtype, _, _) = resolve_row_info(&qid, &ctx);
                    let current = status_of(entry);
                    let final_status = if current == new_status {
                        current.clone()
                    } else {
                        resolve_status(&qtype, &current, new_status)
                    };
                    if final_status != current {
                        let obj = entry.as_object_mut().expect("quest entry is object");
                        let timers = obj
                            .entry("statusTimers".to_string())
                            .or_insert_with(|| Value::Object(Map::new()));
                        if let Some(tmap) = timers.as_object_mut() {
                            // 只增不更：该状态已有时间戳则保留首次（键为状态整数）
                            tmap.entry(timer_key(&final_status))
                                .or_insert_with(|| Value::from(now));
                        }
                        obj.insert("status".to_string(), status_value(&final_status));
                        // 降级到未开始（≤ AvailableForStart）：清 completedConditions
                        //（参考 ProfileSaver.cs：不清理会导致再升级时条件视为已完成）
                        if final_status == "Locked" || final_status == "AvailableForStart" {
                            obj.insert(
                                "completedConditions".to_string(),
                                Value::Array(Vec::new()),
                            );
                        }
                    }
                    if let Some(recipes) = ctx.craft_by_quest.get(&qid) {
                        if current != "Success" && final_status == "Success" {
                            unlock_add.extend(recipes.iter().cloned());
                        } else if current == "Success" && final_status != "Success" {
                            unlock_remove.extend(recipes.iter().cloned());
                        }
                    }
                }
            }
        }
        // 二轮：移除前端没有的任务（删除不动联动配方）
        quests.retain(|e| {
            e.get("qid")
                .and_then(|v| v.as_str())
                .map_or(true, |q| !remove.contains(q))
        });
        // 三轮：新增条目（Locked 起 + 目标状态计时）
        for row in &data.rows {
            if quests.iter().any(|e| {
                e.get("qid").and_then(|v| v.as_str()) == Some(row.qid.as_str())
            }) {
                continue;
            }
            let (qtype, _, _) = resolve_row_info(&row.qid, &ctx);
            let final_status = if legal_statuses(&qtype).contains(&row.status.as_str()) {
                row.status.clone()
            } else {
                resolve_status(&qtype, "Locked", &row.status)
            };
            let mut timers = Map::new();
            timers.insert(timer_key("Locked"), Value::from(now));
            if final_status != "Locked" {
                timers.insert(timer_key(&final_status), Value::from(now));
            }
            new_entries.push(serde_json::json!({
                "qid": row.qid,
                "startTime": 0,
                "status": status_value(&final_status),
                "statusTimers": Value::Object(timers),
                "completedConditions": [],
                "availableAfter": 0,
            }));
        }
        quests.extend(new_entries);
    }

    // 联动配方增量同步 UnlockedInfo.unlockedProductionRecipe（不存在则创建）
    if !unlock_add.is_empty() || !unlock_remove.is_empty() {
        if raw
            .pointer("/characters/pmc/UnlockedInfo/unlockedProductionRecipe")
            .is_none()
        {
            if let Some(pmc) = raw.pointer_mut("/characters/pmc") {
                if let Some(obj) = pmc.as_object_mut() {
                    let info = obj
                        .entry("UnlockedInfo".to_string())
                        .or_insert_with(|| serde_json::json!({}));
                    if let Some(io) = info.as_object_mut() {
                        io.entry("unlockedProductionRecipe".to_string())
                            .or_insert_with(|| Value::Array(Vec::new()));
                    }
                }
            }
        }
        if let Some(list) = raw
            .pointer_mut("/characters/pmc/UnlockedInfo/unlockedProductionRecipe")
            .and_then(|v| v.as_array_mut())
        {
            for rid in &unlock_remove {
                list.retain(|x| x.as_str() != Some(rid.as_str()));
            }
            for rid in &unlock_add {
                if !list.iter().any(|x| x.as_str() == Some(rid.as_str())) {
                    list.push(Value::from(rid.as_str()));
                }
            }
        }
    }

    let out = serde_json::to_string(&raw).map_err(|e| e.to_string())?;
    fs::write(&path, out).map_err(|e| e.to_string())?;
    Ok(SaveResult {
        backup_path: backup_path.map(|p| p.to_string_lossy().to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 4.1.6 真实档：status 是整数；内部名字与写回整数必须一一对应
    #[test]
    fn status_round_trips_server_integer_form() {
        assert_eq!(status_of(&json!({ "status": 4 })), "Success");
        assert_eq!(status_of(&json!({ "status": 1 })), "AvailableForStart");
        assert_eq!(status_value("Success"), json!(4));
        assert_eq!(status_value("Locked"), json!(0));
        assert_eq!(timer_key("AvailableForStart"), "1");
        assert_eq!(timer_key("Success"), "4");
    }

    /// 旧档可能存枚举名字符串，读取路径仍要认
    #[test]
    /// 本机真实库自检（默认忽略）：模板里的条件 id 集与奖励摘要必须能取到 ——
    /// 任务页右栏的「完成条件 3 项 · 已完成 3 项」「+7,200 经验 / +0.06 好感」全靠这两条。
    #[test]
    #[ignore = "需要本机 SPT 安装 E:\\EFT"]
    fn real_quest_templates_expose_conditions_and_rewards() {
        let txt = std::fs::read_to_string(
            r"E:\EFT\SPT_Runtime\SPT_Data\database\templates\quests.json",
        )
        .expect("读 templates/quests.json 失败");
        let v: Value = serde_json::from_str(&txt).unwrap();
        let map = v.as_object().expect("4.1 起应为字典");
        let mut with_cond = 0;
        let mut with_exp = 0;
        for (_qid, quest) in map.iter().take(60) {
            let conds = cond_ids_of(quest);
            if !conds.is_empty() {
                with_cond += 1;
                // 条件 id 必须像 MongoId（24 位十六进制）
                assert_eq!(conds[0].len(), 24, "条件 id 不像 MongoId: {}", conds[0]);
            }
            let (exp, _stand, _money) = reward_summary(quest);
            if exp > 0 {
                with_exp += 1;
            }
        }
        assert!(with_cond > 10, "前 60 个任务只有 {} 个带条件", with_cond);
        assert!(with_exp > 10, "前 60 个任务只有 {} 个带经验奖励", with_exp);
    }

    fn legacy_string_status_still_reads() {
        assert_eq!(status_of(&json!({ "status": "Started" })), "Started");
    }

    /// 越界数值与缺字段都归 Locked（服务器只承认 0..9）
    #[test]
    fn unknown_status_falls_back_to_locked() {
        assert_eq!(status_of(&json!({ "status": 42 })), "Locked");
        assert_eq!(status_of(&json!({})), "Locked");
    }
}
