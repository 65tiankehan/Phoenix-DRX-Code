//! 存档问题检测与修复（13-issues 保存拦截弹窗）——对齐参考编辑器 IssuesService。
//!
//! 数据口径（对照参考 IssuesService.cs / Core/Issues/*.cs /
//! CharacterTraderStandingExtended.cs / CharacterInventory.cs / QuestData.cs）：
//! - 检测顺序：PMC 重复物品 → Scav 重复物品 → 商人等级（TradersInfo 逐商人）→
//!   任务条件（只扫档案中 status >= AvailableForStart 条目的模板 conditions）；
//! - 商人等级：idx = clamp(loyalty-1, 0, 段数-1)，段 minLevel > PMC 等级 → 问题；
//!   段缺失 / 档案 Info.Level 缺失 / 段数为 0 → 跳过（C# null 比较恒 false）；
//! - 任务 Level 条件：value 先钳制到 globals maxLevel（检测与就近值都用钳后值），
//!   compareMethod 判定不满足 PMC 等级（档案缺省按 1）→ 问题；
//! - 任务 Quest 条件：要求状态取 status[0]（空/非法跳过）+ 档案目标任务
//!   （查无跳过），目标状态序 < 要求序 → 问题；
//! - 任务 TraderLoyalty / TraderStanding：目标商人查无跳过；判定不满足 →
//!   就近值（< 1 / < 0.01 跳过）→ 问题；
//! - 修复语义：等级/前置状态直写档案字段（不动 Fence→Scav 同步、不触发
//!   SetLevel，对齐参考两条修复路径）；忠诚修复走 public setter 联动
//!   （等级钳制 + 销售额/声望按下限与起始捕获值联动 + 解锁标志）；
//!   声望修复直写内部字段（绕过联动）；重复物品 = 按物品 _id 分组计数 >1，
//!   BFS 展开子树（parentId 找子）后整体移除。
//! 写回铁律：先备份，后写盘；修复只改内存态，选中问题全部生效后落盘一次。

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::Path;

use serde::Serialize;
use serde_json::Value;
use tauri::AppHandle;

use crate::backup::backup_before_write;
use crate::character::database_dir;
use crate::profile::{is_safe_id, profiles_dir};

// ———————— 类型 ————————

/// 单条存档问题（id 供逐条修复定位；描述字段由前端按 kind 组装文案）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueRow {
    pub id: String,
    /// dup / trader-level / quest-level / quest-status / quest-loyalty / quest-standing
    pub kind: String,
    /// dup：PMC / Scav
    pub side: Option<String>,
    /// trader-level：涉事商人名
    pub trader_name: Option<String>,
    /// quest-*：任务名
    pub quest_name: Option<String>,
    /// quest-*：任务所属商人名
    pub quest_trader_name: Option<String>,
    /// quest-status：前置任务名
    pub target_quest_name: Option<String>,
    /// quest-status / quest-loyalty / quest-standing：目标商人名
    pub target_trader_name: Option<String>,
    /// 数值或状态枚举名（quest-status 的 Success 等由前端本地化）
    pub required_text: Option<String>,
}

/// 修复结果：写盘后重扫的剩余问题 + 备份文件路径
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssuesFixResult {
    pub remaining: Vec<IssueRow>,
    pub backup_path: Option<String>,
}

/// 商人上下文（检测 + 修复共用）
#[derive(Clone)]
struct TraderCtx {
    /// base.json nickname（locale 缺失时兜底）
    nickname: String,
    min_levels: Vec<i64>,
    min_sales: Vec<f64>,
    min_standing: Vec<f64>,
}

/// 任务模板片段（检测 + 修复共用）
#[derive(Clone)]
struct QuestTpl {
    trader_id: String,
    conditions: Vec<Value>,
}

/// 问题上下文：locale + globals 等级/经验表 + 任务模板 + 商人段表
struct IssuesCtx {
    locale: HashMap<String, String>,
    max_level: i64,
    /// globals 经验表（config/exp/level/expTable 每项 exp），等级修复联动经验对齐
    exp_table: Vec<i64>,
    quests: HashMap<String, QuestTpl>,
    traders: HashMap<String, TraderCtx>,
}

// ———————— 语义辅助（QuestData.cs / CharacterTraderStandingExtended.cs） ————————

/// 档案侧 status 兼容两种序列化：4.1.6 为整数（4），旧档为字符串（"Success"）
fn status_text(v: Option<&Value>) -> String {
    match v {
        Some(Value::Number(n)) => status_name(n.as_i64().unwrap_or(-1)),
        Some(Value::String(s)) => s.clone(),
        _ => String::new(),
    }
}

fn status_name(n: i64) -> String {
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
        _ => "",
    }
    .to_string()
}

/// QuestStatus 数值序（声明序，即参考枚举比较基准）
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

/// CheckRequiredValue（QuestData.cs）：compareMethod 比较判定，未知方法按满足
fn check_required(value: i64, method: &str, current: i64) -> bool {
    match method {
        ">=" => current >= value,
        ">" => current > value,
        "<=" => current <= value,
        "<" => current < value,
        "!=" => current != value,
        "==" => current == value,
        _ => true,
    }
}

/// GetNearestValue（QuestData.cs）：">"/"!=" 加 1；"<" 减 1；其余原值
fn nearest_value(value: i64, method: &str) -> i64 {
    match method {
        ">" | "!=" => value + 1,
        "<" => value - 1,
        _ => value,
    }
}

/// GetNearestFloatValue（QuestData.cs）：">"/"!=" 加 0.01；"<" 减 0.01；其余原值
/// （条件 value 为整数，按 f64 带两位小数参与比较）
fn nearest_float_value(value: i64, method: &str) -> f64 {
    match method {
        ">" | "!=" => value as f64 + 0.01,
        "<" => value as f64 - 0.01,
        _ => value as f64,
    }
}

/// 声望保留 2 位小数（traders.rs 同款）
fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// 商人显示名：locale "{tid} nickname" → base nickname → 空串（前端回退本地名）
fn trader_display_name(ctx: &IssuesCtx, tid: &str) -> String {
    if let Some(n) = ctx.locale.get(&format!("{} nickname", tid)) {
        return n.clone();
    }
    ctx.traders
        .get(tid)
        .map(|t| t.nickname.clone())
        .unwrap_or_default()
}

/// 任务显示名：locale "{qid} name" → qid
fn quest_display_name(ctx: &IssuesCtx, qid: &str) -> String {
    ctx.locale
        .get(&format!("{} name", qid))
        .cloned()
        .unwrap_or_else(|| qid.to_string())
}

// ———————— 上下文装载 ————————

/// 从服务器数据目录读齐问题上下文；locale 按 language 回退 en
fn load_ctx(app: &AppHandle, language: &str) -> IssuesCtx {
    let mut ctx = IssuesCtx {
        locale: HashMap::new(),
        max_level: 0,
        exp_table: Vec::new(),
        quests: HashMap::new(),
        traders: HashMap::new(),
    };
    let Some(dir) = database_dir(app) else {
        return ctx;
    };
    let db = Path::new(&dir);

    // globals.json：PMC 等级上限（config/exp/level/maxLevel）+ 经验表（config/exp/level/expTable）
    if let Ok(txt) = fs::read_to_string(db.join("globals.json")) {
        if let Ok(g) = serde_json::from_str::<Value>(&txt) {
            // 4.1.6 真实键名为 snake_case 的 `exp_table`（元素 {exp: N}），且没有 maxLevel：
            // 等级上限 = exp_table.len() - 1（79 项 → 0..78 级）。
            let lvl = g.pointer("/config/exp/level");
            ctx.exp_table = lvl
                .and_then(|v| v.get("exp_table").or_else(|| v.get("expTable")))
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .map(|e| e.get("exp").and_then(|x| x.as_i64()).unwrap_or(0))
                        .collect()
                })
                .unwrap_or_default();
            ctx.max_level = lvl
                .and_then(|v| v.get("maxLevel"))
                .and_then(|v| v.as_i64())
                .or_else(|| {
                    if ctx.exp_table.is_empty() {
                        None
                    } else {
                        Some(ctx.exp_table.len() as i64 - 1)
                    }
                })
                .unwrap_or(0);
        }
    }

    // templates/quests.json：qid → traderId + conditions
    // 4.1.6 真实形态是字典 {qid: {...}}，旧档是数组 [{_id, ...}]；
    // 条目内 conditions 是 {AvailableForStart:[], AvailableForFinish:[], Fail:[]} 三桶，不是单数组。
    if let Ok(txt) = fs::read_to_string(db.join("templates").join("quests.json")) {
        if let Ok(doc) = serde_json::from_str::<Value>(&txt) {
            let entries: Vec<(String, &Value)> = match &doc {
                Value::Object(map) => map.iter().map(|(k, v)| (k.clone(), v)).collect(),
                Value::Array(arr) => arr
                    .iter()
                    .filter_map(|t| {
                        t.get("_id")
                            .and_then(|v| v.as_str())
                            .map(|id| (id.to_string(), t))
                    })
                    .collect(),
                _ => Vec::new(),
            };
            for (qid, t) in entries {
                let conditions: Vec<Value> = match t.get("conditions") {
                    Some(Value::Array(a)) => a.clone(),
                    Some(Value::Object(m)) => m
                        .values()
                        .filter_map(|v| v.as_array())
                        .flat_map(|a| a.iter().cloned())
                        .collect(),
                    _ => Vec::new(),
                };
                ctx.quests.insert(
                    qid,
                    QuestTpl {
                        trader_id: t
                            .get("traderId")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        conditions,
                    },
                );
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

/// 补齐商人上下文：档案 TradersInfo 逐商人 ∩ traders/{tid}/base.json
/// （min_levels / min_sales / min_standing 取各等级段下限）
fn load_traders_ctx(app: &AppHandle, pmc: &Value, ctx: &mut IssuesCtx) {
    let Some(map) = pmc.get("TradersInfo").and_then(|v| v.as_object()) else {
        return;
    };
    let Some(dir) = database_dir(app) else {
        return;
    };
    for (tid, _) in map {
        let path = Path::new(&dir).join("traders").join(tid).join("base.json");
        let Ok(txt) = fs::read_to_string(path) else {
            continue;
        };
        let Ok(base) = serde_json::from_str::<Value>(&txt) else {
            continue;
        };
        let segs = base
            .get("loyaltyLevels")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        ctx.traders.insert(
            tid.clone(),
            TraderCtx {
                nickname: base
                    .get("nickname")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                min_levels: segs
                    .iter()
                    .map(|s| s.get("minLevel").and_then(|v| v.as_i64()).unwrap_or(0))
                    .collect(),
                min_sales: segs
                    .iter()
                    .map(|s| s.get("minSalesSum").and_then(|v| v.as_f64()).unwrap_or(0.0))
                    .collect(),
                min_standing: segs
                    .iter()
                    .map(|s| s.get("minStanding").and_then(|v| v.as_f64()).unwrap_or(0.0))
                    .collect(),
            },
        );
    }
}

// ———————— 扫描（IssuesService.GetIssues 语义） ————————

/// 空问题行骨架（后按类型补描述字段）
fn issue(id: &str, kind: &str) -> IssueRow {
    IssueRow {
        id: id.to_string(),
        kind: kind.to_string(),
        side: None,
        trader_name: None,
        quest_name: None,
        quest_trader_name: None,
        target_quest_name: None,
        target_trader_name: None,
        required_text: None,
    }
}

/// 重复物品 ID（每侧一条；对齐 C# GroupedInventory.Any）
/// 分组按物品 _id 计数 >1，不排除空 id
fn dup_row(side: &str, items: Option<&Vec<Value>>) -> Option<IssueRow> {
    let items = items?;
    let mut seen: HashSet<&str> = HashSet::new();
    for it in items {
        if let Some(id) = it.get("_id").and_then(|v| v.as_str()) {
            if !seen.insert(id) {
                let mut r = issue(&format!("dup-{}", side.to_ascii_lowercase()), "dup");
                r.side = Some(side.to_string());
                return Some(r);
            }
        }
    }
    None
}

/// 全量扫描：重复物品（PMC → Scav）→ 商人等级 → 任务四类条件
fn scan_issues(raw: &Value, ctx: &IssuesCtx) -> Vec<IssueRow> {
    let mut rows = Vec::new();
    let Some(pmc) = raw.pointer("/characters/pmc") else {
        return rows;
    };

    if let Some(r) = dup_row(
        "PMC",
        pmc.pointer("/Inventory/items").and_then(|v| v.as_array()),
    ) {
        rows.push(r);
    }
    if let Some(r) = dup_row(
        "Scav",
        raw.pointer("/characters/scav/Inventory/items")
            .and_then(|v| v.as_array()),
    ) {
        rows.push(r);
    }

    let level = pmc.pointer("/Info/Level").and_then(|v| v.as_i64());

    // 档案商人四字段速查（loyalty / standing）
    let mut loyalty_of: HashMap<&str, i64> = HashMap::new();
    let mut standing_of: HashMap<&str, f64> = HashMap::new();
    if let Some(tmap) = pmc.get("TradersInfo").and_then(|v| v.as_object()) {
        for (tid, info) in tmap {
            loyalty_of.insert(
                tid.as_str(),
                info.get("loyaltyLevel").and_then(|v| v.as_i64()).unwrap_or(0),
            );
            standing_of.insert(
                tid.as_str(),
                info.get("standing").and_then(|v| v.as_f64()).unwrap_or(0.0),
            );
        }
    }

    // —— 商人等级：段 minLevel > PMC 等级 → 问题（HasLevelIssue 语义）
    if let (Some(tmap), Some(level)) = (pmc.get("TradersInfo").and_then(|v| v.as_object()), level) {
        for (tid, info) in tmap {
            let Some(t) = ctx.traders.get(tid) else {
                continue;
            };
            let loyalty = info
                .get("loyaltyLevel")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let max_level = (t.min_levels.len() as i64).max(loyalty);
            if max_level <= 0 {
                continue;
            }
            let idx = (loyalty - 1).max(0).min(max_level - 1);
            let Some(&min_level) = t.min_levels.get(idx.max(0) as usize) else {
                continue;
            };
            if min_level > level {
                let mut r = issue(&format!("trader-level:{}", tid), "trader-level");
                r.trader_name = Some(trader_display_name(ctx, tid));
                r.required_text = Some(min_level.to_string());
                rows.push(r);
            }
        }
    }

    // —— 任务条件：只扫 status >= AvailableForStart 的档案条目
    let Some(quests) = pmc.get("Quests").and_then(|v| v.as_array()) else {
        return rows;
    };
    let status_of = |qid: &str| -> String {
        quests
            .iter()
            .find(|q| q.get("qid").and_then(|v| v.as_str()) == Some(qid))
            .map(|q| status_text(q.get("status")))
            .unwrap_or_default()
    };
    for q in quests {
        let Some(qid) = q.get("qid").and_then(|v| v.as_str()) else {
            continue;
        };
        if status_num(&status_text(q.get("status"))) < 1 {
            continue;
        }
        let Some(tpl) = ctx.quests.get(qid) else {
            continue;
        };
        let quest_name = quest_display_name(ctx, qid);
        let quest_trader = trader_display_name(ctx, &tpl.trader_id);
        for (idx, cond) in tpl.conditions.iter().enumerate() {
            let ctype = cond
                .get("conditionType")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let method = cond
                .get("compareMethod")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let value = cond.get("value").and_then(|v| v.as_i64()).unwrap_or(0);
            match ctype {
                // Level 条件：value 先钳制到 globals 上限（检测与就近值都用钳后值）
                "Level" => {
                    let v = if ctx.max_level > 0 {
                        value.min(ctx.max_level)
                    } else {
                        value
                    };
                    if !check_required(v, method, level.unwrap_or(1)) {
                        let mut r = issue(&format!("quest-level:{}:{}", qid, idx), "quest-level");
                        r.quest_name = Some(quest_name.clone());
                        r.quest_trader_name = Some(quest_trader.clone());
                        r.required_text = Some(nearest_value(v, method).to_string());
                        rows.push(r);
                    }
                }
                // 前置任务状态：status[0] + 档案目标任务（查无跳过）
                "Quest" => {
                    let Some(req) = cond
                        .get("status")
                        .and_then(|v| v.as_array())
                        .and_then(|a| a.first())
                        .and_then(|v| v.as_str())
                    else {
                        continue;
                    };
                    if status_num(req) < 0 {
                        continue;
                    }
                    let target_qid = cond.get("target").and_then(|v| v.as_str()).unwrap_or("");
                    let tstatus = status_of(target_qid);
                    if tstatus.is_empty() {
                        continue;
                    }
                    if status_num(&tstatus) < status_num(req) {
                        let mut r =
                            issue(&format!("quest-status:{}:{}", qid, target_qid), "quest-status");
                        r.quest_name = Some(quest_name.clone());
                        r.quest_trader_name = Some(quest_trader.clone());
                        r.target_quest_name = Some(quest_display_name(ctx, target_qid));
                        r.target_trader_name = Some(trader_display_name(
                            ctx,
                            ctx.quests
                                .get(target_qid)
                                .map(|t| t.trader_id.as_str())
                                .unwrap_or(""),
                        ));
                        r.required_text = Some(req.to_string());
                        rows.push(r);
                    }
                }
                // 商人忠诚度：目标商人查无跳过；就近值 < 1 跳过
                "TraderLoyalty" => {
                    let Some(tid) = cond.get("target").and_then(|v| v.as_str()) else {
                        continue;
                    };
                    if !ctx.traders.contains_key(tid) {
                        continue;
                    }
                    let loyalty = loyalty_of.get(tid).copied().unwrap_or(0);
                    if !check_required(value, method, loyalty) {
                        let req = nearest_value(value, method);
                        if req < 1 {
                            continue;
                        }
                        let mut r =
                            issue(&format!("quest-loyalty:{}:{}", qid, tid), "quest-loyalty");
                        r.quest_name = Some(quest_name.clone());
                        r.quest_trader_name = Some(quest_trader.clone());
                        r.target_trader_name = Some(trader_display_name(ctx, tid));
                        r.required_text = Some(req.to_string());
                        rows.push(r);
                    }
                }
                // 商人声望：判定取信任度截断；就近值 < 0.01 跳过
                "TraderStanding" => {
                    let Some(tid) = cond.get("target").and_then(|v| v.as_str()) else {
                        continue;
                    };
                    if !ctx.traders.contains_key(tid) {
                        continue;
                    }
                    let standing = standing_of.get(tid).copied().unwrap_or(0.0);
                    if !check_required(value, method, standing.trunc() as i64) {
                        let req = nearest_float_value(value, method);
                        if req < 0.01 {
                            continue;
                        }
                        let mut r =
                            issue(&format!("quest-standing:{}:{}", qid, tid), "quest-standing");
                        r.quest_name = Some(quest_name.clone());
                        r.quest_trader_name = Some(quest_trader.clone());
                        r.target_trader_name = Some(trader_display_name(ctx, tid));
                        r.required_text = Some(format!("{:.2}", req));
                        rows.push(r);
                    }
                }
                _ => {}
            }
        }
    }
    rows
}

/// 问题清单（读档 → 上下文 → 扫描）；档案缺失或不可读时为空清单
pub fn issues_scan(app: &AppHandle, id: &str) -> Result<Vec<IssueRow>, String> {
    if !is_safe_id(id) {
        return Ok(Vec::new());
    }
    let dir = profiles_dir(app).ok_or_else(|| "未选择 SPT 服务器".to_string())?;
    let raw: Value = serde_json::from_str(
        &fs::read_to_string(Path::new(&dir).join(format!("{}.json", id)))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let language = crate::server::data_language(app, Some(&raw));
    let mut ctx = load_ctx(app, &language);
    if let Some(pmc) = raw.pointer("/characters/pmc") {
        load_traders_ctx(app, pmc, &mut ctx);
    }
    Ok(scan_issues(&raw, &ctx))
}

// ———————— 修复（Core/Issues/*.cs 的 FixAction 语义） ————————

/// PMC 等级修复：复刻 CharacterInfo.Level setter 联动（PMCLevelIssue.FixAction）
/// 写盘等级钳到 [1, 经验表长]；经验不在新等级区间 [expStart, expEnd) 时对齐 expStart
fn set_pmc_level(info: &mut Value, req: i64, exp_table: &[i64]) {
    let cur = info.get("Level").and_then(|v| v.as_i64()).unwrap_or(1);
    if cur == req {
        return; // C# setter 有 if (level != value) 守卫
    }
    // 经验表缺失（服务器数据异常）：C# 此分支返回 1 属崩溃态，保守只写等级不动经验
    if exp_table.is_empty() {
        info["Level"] = serde_json::json!(req);
        return;
    }
    let len = exp_table.len() as i64;
    let level = req.clamp(1, len);
    info["Level"] = serde_json::json!(level);
    // 新等级的经验区间：expStart = 前序档累计；expEnd = 下一档（末档 +100000）
    let exp_start: i64 = exp_table[..level as usize].iter().sum();
    let exp_end = exp_start + exp_table.get(level as usize).copied().unwrap_or(100_000);
    let in_range = info
        .get("Experience")
        .and_then(|v| v.as_i64())
        .map(|e| e >= exp_start && e < exp_end)
        .unwrap_or(false);
    if !in_range {
        info["Experience"] = serde_json::json!(exp_start);
    }
}

/// 商人忠诚度修复：复刻 CharacterTraderStandingExtended.LoyaltyLevel setter 联动
/// （TraderLoyaltyIssue.FixAction）——等级钳到 [1, max(段数, 当前)]，
/// 销售额/声望对齐新等级段要求（升档取大、降档取小，以修复前快照托底），1 级以上解锁
fn apply_loyalty_setter(t: &mut Value, seg: &TraderCtx, req: i64) {
    let level_start = t.get("loyaltyLevel").and_then(|v| v.as_i64()).unwrap_or(1);
    let max_level = (seg.min_levels.len() as i64).max(level_start).max(1);
    let level = req.clamp(1, max_level);
    t["loyaltyLevel"] = serde_json::json!(level);

    // SetSalesSum：销售额为整数（C# long）
    let sales_start = t.get("salesSum").and_then(|v| v.as_i64()).unwrap_or(0);
    let min_sales = seg
        .min_sales
        .get((level - 1).max(0) as usize)
        .copied()
        .map(|v| v as i64)
        .unwrap_or(sales_start);
    t["salesSum"] = serde_json::json!(if level >= level_start {
        min_sales.max(sales_start)
    } else {
        min_sales.min(sales_start)
    });

    // SetStanding：声望为浮点（C# float），直写内部字段不触发 Fence 同步/SetLevel
    let standing_start = t.get("standing").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let min_standing = seg
        .min_standing
        .get((level - 1).max(0) as usize)
        .copied()
        .unwrap_or(standing_start);
    t["standing"] = serde_json::json!(round2(if level >= level_start {
        min_standing.max(standing_start)
    } else {
        min_standing.min(standing_start)
    }));

    // SetUnlocked：1 级以上必解锁（原有 true 不收回）
    let unlocked_start = t.get("unlocked").and_then(|v| v.as_bool()).unwrap_or(false);
    t["unlocked"] = serde_json::json!(level > 1 || unlocked_start);
}

/// 重复物品修复：复刻 CharacterInventory.RemoveDuplicatedItems 的删除语义
/// （DuplicateItemsIDIssue.FixAction）——对每个重复 _id：BFS 收集其子树全部 id 一并移除。
/// C# FinalRemoveItems「找到就删、不弹队列头」净效果为集合全删，故 HashSet 去重 + 防环等价；
/// 空 _id 成组但不作删除根（对齐 GetCompleteItemsList 的 IsNullOrEmpty 跳过），空子保守保留
fn remove_duplicated_items(items: &mut Vec<Value>) -> bool {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for it in items.iter() {
        let k = it.get("_id").and_then(|v| v.as_str()).unwrap_or("");
        *counts.entry(k).or_insert(0) += 1;
    }
    let dup_keys: Vec<&str> = counts
        .iter()
        .filter(|(k, c)| **c > 1 && !k.is_empty())
        .map(|(k, _)| *k)
        .collect();
    if dup_keys.is_empty() {
        return false;
    }

    let mut doomed: HashSet<String> = HashSet::new();
    for key in dup_keys {
        let mut queue = VecDeque::new();
        queue.push_back(key.to_string());
        while let Some(cur) = queue.pop_front() {
            if !doomed.insert(cur.clone()) {
                continue; // 已收集过（防环、防重复展开）
            }
            for it in items.iter() {
                if it.get("parentId").and_then(|v| v.as_str()) == Some(cur.as_str()) {
                    if let Some(id) = it.get("_id").and_then(|v| v.as_str()) {
                        if !id.is_empty() {
                            queue.push_back(id.to_string());
                        }
                    }
                }
            }
        }
    }
    let before = items.len();
    items.retain(|it| {
        it.get("_id")
            .and_then(|v| v.as_str())
            .map(|k| !doomed.contains(k))
            .unwrap_or(true) // 无 _id 的物品不动
    });
    items.len() != before
}

/// 修复选中问题：读档 → 重扫定位 → 按 id 匹配应用 → 备份写盘 → 重扫剩余。
/// required 参数直接取检测时算好的 required_text；无修复不改盘（remaining = 当前全部）
pub fn issues_fix(app: &AppHandle, id: &str, issue_ids: &[String]) -> Result<IssuesFixResult, String> {
    if !is_safe_id(id) || issue_ids.is_empty() {
        return Ok(IssuesFixResult {
            remaining: Vec::new(),
            backup_path: None,
        });
    }
    let dir = profiles_dir(app).ok_or_else(|| "未选择 SPT 服务器".to_string())?;
    let path = Path::new(&dir).join(format!("{}.json", id));
    let mut raw: Value = serde_json::from_str(&fs::read_to_string(&path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let language = crate::server::data_language(app, Some(&raw));
    let mut ctx = load_ctx(app, &language);
    if let Some(pmc) = raw.pointer("/characters/pmc") {
        load_traders_ctx(app, pmc, &mut ctx);
    }

    // 重扫定位：required_text 即检测时算好的修复参数，选中集按 id 匹配
    let rows = scan_issues(&raw, &ctx);
    let selected: HashSet<&String> = issue_ids.iter().collect();
    let mut characters = raw.get_mut("characters");
    let mut changed = false;

    for row in rows.iter() {
        if !selected.contains(&row.id) {
            continue;
        }
        match row.kind.as_str() {
            // PMCLevelIssue / QuestLevelIssue：同走 PMC 等级联动（等级 + 经验对齐）
            "trader-level" | "quest-level" => {
                let Some(req) = row.required_text.as_deref().and_then(|s| s.parse::<i64>().ok())
                else {
                    continue;
                };
                if let Some(info) = characters
                    .as_mut()
                    .and_then(|c| c.get_mut("pmc"))
                    .and_then(|p| p.get_mut("Info"))
                {
                    set_pmc_level(info, req, &ctx.exp_table);
                    changed = true;
                }
            }
            // QuestStatusIssue：写状态枚举名 + statusTimers 补键（C# Status setter 联动）
            "quest-status" => {
                let Some(target_qid) = row.id.rsplit(':').next().map(|s| s.to_string()) else {
                    continue;
                };
                let Some(req_status) = row.required_text.as_deref() else {
                    continue;
                };
                if let Some(quests) = characters
                    .as_mut()
                    .and_then(|c| c.get_mut("pmc"))
                    .and_then(|p| p.get_mut("Quests"))
                    .and_then(|q| q.as_array_mut())
                {
                    for q in quests.iter_mut() {
                        // 任务条目主键是 qid（CharacterQuest.cs），不是 _id——此前永不命中
                        if q.get("qid").and_then(|v| v.as_str()) != Some(target_qid.as_str()) {
                            continue;
                        }
                        if let Some(obj) = q.as_object_mut() {
                            obj.insert("status".into(), serde_json::json!(req_status));
                            let timers = obj
                                .entry("statusTimers")
                                .or_insert_with(|| serde_json::json!({}));
                            if let Some(map) = timers.as_object_mut() {
                                // C# GetTimestamp：Unix 秒（double）
                                let now = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_secs_f64())
                                    .unwrap_or(0.0);
                                map.entry(req_status.to_string())
                                    .or_insert_with(|| serde_json::json!(now));
                            }
                            changed = true;
                        }
                        break;
                    }
                }
            }
            // QuestTraderLoyaltyIssue：走 public setter 联动（等级钳制 + 三联动）
            "quest-loyalty" => {
                let Some(tid) = row.id.rsplit(':').next().map(|s| s.to_string()) else {
                    continue;
                };
                let Some(req) = row.required_text.as_deref().and_then(|s| s.parse::<i64>().ok())
                else {
                    continue;
                };
                let Some(seg) = ctx.traders.get(tid.as_str()).cloned() else {
                    continue;
                };
                if let Some(t) = characters
                    .as_mut()
                    .and_then(|c| c.get_mut("pmc"))
                    .and_then(|p| p.get_mut("TradersInfo"))
                    .and_then(|ts| ts.as_object_mut())
                    .and_then(|ts| ts.get_mut(tid.as_str()))
                {
                    apply_loyalty_setter(t, &seg, req);
                    changed = true;
                }
            }
            // QuestTraderStandingIssue：直写内部字段（绕过 public setter 联动）
            "quest-standing" => {
                let Some(tid) = row.id.rsplit(':').next().map(|s| s.to_string()) else {
                    continue;
                };
                let Some(req) = row.required_text.as_deref().and_then(|s| s.parse::<f64>().ok())
                else {
                    continue;
                };
                if let Some(t) = characters
                    .as_mut()
                    .and_then(|c| c.get_mut("pmc"))
                    .and_then(|p| p.get_mut("TradersInfo"))
                    .and_then(|ts| ts.as_object_mut())
                    .and_then(|ts| ts.get_mut(tid.as_str()))
                {
                    t["standing"] = serde_json::json!(round2(req));
                    changed = true;
                }
            }
            // DuplicateItemsIDIssue：BFS 展开子树后整体移除（Inventory.items 小写，与存档一致）
            "dup" if row.side.as_deref() == Some("PMC") => {
                if let Some(items) = characters
                    .as_mut()
                    .and_then(|c| c.get_mut("pmc"))
                    .and_then(|p| p.pointer_mut("/Inventory/items"))
                    .and_then(|v| v.as_array_mut())
                {
                    if remove_duplicated_items(items) {
                        changed = true;
                    }
                }
            }
            "dup" if row.side.as_deref() == Some("Scav") => {
                if let Some(items) = characters
                    .as_mut()
                    .and_then(|c| c.get_mut("scav"))
                    .and_then(|p| p.pointer_mut("/Inventory/items"))
                    .and_then(|v| v.as_array_mut())
                {
                    if remove_duplicated_items(items) {
                        changed = true;
                    }
                }
            }
            _ => {}
        }
    }

    let mut backup_path: Option<std::path::PathBuf> = None;
    let remaining = if changed {
        // 写回铁律：先备份，后写盘（单次落盘，对齐 quests.rs 先例）
        backup_path = backup_before_write(app, &path)?;
        let out = serde_json::to_string(&raw).map_err(|e| e.to_string())?;
        fs::write(&path, out).map_err(|e| e.to_string())?;
        scan_issues(&raw, &ctx)
    } else {
        rows
    };
    Ok(IssuesFixResult {
        remaining,
        backup_path: backup_path.map(|p| p.to_string_lossy().to_string()),
    })
}
