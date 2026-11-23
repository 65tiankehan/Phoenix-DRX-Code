//! AI 助手执行器（稿图 29，0.2.0 第 4 段）。
//! 白名单 v1 六类操作（方案 §2.2），每条 = serde tag 结构，Rust 侧逐条校验后执行；
//! 执行全部复用各域已验证的 save 链（各自带 backup_before_write）——AI 说不算、执行器说了算。
//! 上下文组装（assistant_context）：只发模型需要的最小目录（技能/商人/任务名/服装/军械库 id 表），
//! 不上传完整存档（隐私边界铁律）。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use tauri::AppHandle;

use crate::arsenal::{arsenal_load, ArsenalSet};
use crate::character::database_dir;
use crate::clothing::{clothing_load, clothing_save};
use crate::profile::{get_current_profile_id, profiles_dir};
use crate::quests::{save_quests, QuestsSaveData};
use crate::skills::{load_skills, save_skills, SkillsSaveData, SkillsSaveRow};
use crate::traders::{load_traders, save_traders, TradersSaveData, TraderSaveRow};

// ----- 白名单操作定义（前端预览卡 ↔ 执行器共用） -----

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum AiOp {
    /// 全部战斗与身体素质技能设为 N 级（1..=51，Progress=N*100 夹 cap；跳过 bot 技能）
    SkillsAll { level: i64 },
    /// 全部商人忠诚等级 4 + 好感 1.0（销售额与解锁态不动）
    TradersMax,
    /// 按任务 qid 完成任务（save_quests 全量差分，含配方联动与备份）
    QuestsComplete { qids: Vec<String> },
    /// 货币调整到目标值（叠加/削减现有堆；目标可为任意正数）
    MoneySet { rub: Option<i64>, eur: Option<i64>, usd: Option<i64> },
    /// 服装全解锁（可选同时换头型/语音）
    ClothingUnlockAll { head: Option<String>, voice: Option<String> },
    /// 换形象（头型/语音，不动服装）
    SetLook { head: Option<String>, voice: Option<String> },
    /// 应用军械库套装
    ArsenalApply { set_id: String, weapon_only: bool },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpResult {
    index: usize,
    ok: bool,
    error: Option<String>,
}

fn profile_id(app: &AppHandle) -> Result<String, String> {
    get_current_profile_id(app).ok_or_else(|| "未选择存档".to_string())
}

/// 最小上下文：模型做规划需要的目录表（id/名称对），不含存档数值明细
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantContext {
    version: String,
    /// 技能 id 列表（普通技能，供点名）；说「全部」时无需点名
    skills: Vec<String>,
    /// 商人 id 列表
    traders: Vec<String>,
    /// 任务 qid + 本地化名（名称匹配 → qid 由模型查此表）
    quests: Vec<(String, String)>,
    /// 服装套装名列表
    suits: Vec<String>,
    /// 军械库套餐 id + 名称
    arsenal: Vec<(String, String)>,
    /// 头型/语音候选名
    heads: Vec<String>,
    voices: Vec<String>,
}

#[tauri::command]
pub async fn assistant_context(app: AppHandle) -> Result<AssistantContext, String> {
    {
        let id = profile_id(&app)?;
        let version = env!("CARGO_PKG_VERSION").to_string();

        let skills = load_skills(&app, &id)
            .map(|d| {
                d.pmc
                    .common
                    .iter()
                    .filter(|r| !r.bot)
                    .map(|r| r.id.clone())
                    .collect()
            })
            .unwrap_or_default();

        let traders = load_traders(&app, &id)
            .map(|d| d.traders.iter().map(|r| r.id.clone()).collect())
            .unwrap_or_default();

        // 任务名表：档案已有 + 缺失（模板全集减事件任务），名称取 locales（quests.rs 同口径）
        let mut quests: Vec<(String, String)> = Vec::new();
        if let Some(db) = database_dir(&app) {
            if let Ok(txt) = fs::read_to_string(std::path::Path::new(&db).join("locales").join("global").join("en.json")) {
                let names: Value = serde_json::from_str(&txt).unwrap_or(Value::Null);
                if let Ok(txt2) = fs::read_to_string(std::path::Path::new(&db).join("quests.json")) {
                    if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&txt2) {
                        for qid in map.keys() {
                            let name = names
                                .get(format!("{} name", qid))
                                .or_else(|| names.get(format!("{} Name", qid)))
                                .and_then(|v| v.as_str())
                                .unwrap_or(qid);
                            quests.push((qid.clone(), name.to_string()));
                        }
                    }
                }
            }
        }

        let suits_data = clothing_load(app.clone()).await?;
        let suits: Vec<String> = suits_data.suits.iter().map(|s| s.name.clone()).collect();
        let heads = suits_data.heads.iter().map(|h| h.name.clone()).collect();
        let voices = suits_data.voices.iter().map(|v| v.name.clone()).collect();

        let arsenal_sets: Vec<ArsenalSet> = arsenal_load(app.clone()).await?;
        let arsenal = arsenal_sets.iter().map(|s| (s.id.clone(), s.name.clone())).collect();

        Ok(AssistantContext { version, skills, traders, quests, suits, arsenal, heads, voices })
    }
}

/// 执行白名单操作批次：逐条执行，任何一条失败不中断其余（结果逐条回传前端回执）
#[tauri::command]
pub async fn assistant_execute(app: AppHandle, ops: Vec<AiOp>) -> Result<Vec<OpResult>, String> {
    let mut results = Vec::with_capacity(ops.len());
    for (i, op) in ops.into_iter().enumerate() {
        let r = run_op(&app, &op).await;
        results.push(OpResult { index: i, ok: r.is_ok(), error: r.err() });
    }
    Ok(results)
}

async fn run_op(app: &AppHandle, op: &AiOp) -> Result<(), String> {
    let id = profile_id(app)?;
    match op {
        AiOp::SkillsAll { level } => {
            let level = (*level).clamp(1, 51);
            let data = load_skills(app, &id).ok_or("技能数据读取失败")?;
            let mut rows = Vec::new();
            let mut push = |list: &Vec<crate::skills::SkillRow>, group: &str| {
                for r in list {
                    if r.bot {
                        continue;
                    }
                    rows.push(SkillsSaveRow {
                        side: "pmc".into(),
                        group: group.into(),
                        id: r.id.clone(),
                        progress: (level * 100).min(r.cap),
                    });
                }
            };
            push(&data.pmc.common, "Common");
            push(&data.pmc.mastering, "Mastering");
            save_skills(app, &id, &SkillsSaveData { rows })?;
            Ok(())
        }
        AiOp::TradersMax => {
            let data = load_traders(app, &id).ok_or("商人数据读取失败")?;
            let traders: Vec<TraderSaveRow> = data
                .traders
                .iter()
                .map(|r| TraderSaveRow {
                    id: r.id.clone(),
                    loyalty_level: 4,
                    sales_sum: r.sales_sum,
                    standing: r.standing.max(1.0),
                    unlocked: r.unlocked,
                })
                .collect();
            save_traders(
                app,
                &id,
                &TradersSaveData { traders, ragfair: data.ragfair.rating },
            )?;
            Ok(())
        }
        AiOp::QuestsComplete { qids } => {
            if qids.is_empty() {
                return Err("任务列表为空".into());
            }
            let rows = qids.iter().map(|q| crate::quests::QuestSaveRow { qid: q.clone(), status: "Success".into() }).collect();
            save_quests(app, &id, &QuestsSaveData { rows })?;
            Ok(())
        }
        AiOp::MoneySet { rub, eur, usd } => {
            let mut ops: Vec<(&str, i64)> = Vec::new();
            if let Some(v) = rub { ops.push(("5449016a4bdc2d6f028b456f", (*v).max(0))); }
            if let Some(v) = eur { ops.push(("569668774bdc2da229b4568", (*v).max(0))); }
            if let Some(v) = usd { ops.push(("5696686a4bdc2da3298b456a", (*v).max(0))); }
            if ops.is_empty() {
                return Err("没有要调整的货币".into());
            }
            for (tpl, target) in ops {
                set_money(app, tpl, target)?;
            }
            Ok(())
        }
        AiOp::ClothingUnlockAll { head, voice } => {
            let d = clothing_load(app.clone()).await?;
            let owned: Vec<String> = d.suits.iter().map(|s| s.suite_id.clone()).collect();
            clothing_save(
                app.clone(),
                owned,
                head.clone().unwrap_or_else(|| d.current_head.clone()),
                voice.clone().unwrap_or_else(|| d.current_voice.clone()),
            )
            .await?;
            Ok(())
        }
        AiOp::SetLook { head, voice } => {
            let d = clothing_load(app.clone()).await?;
            if head.is_none() && voice.is_none() {
                return Err("头型与语音都为空".into());
            }
            let owned: Vec<String> = d.suits.iter().filter(|s| s.owned).map(|s| s.suite_id.clone()).collect();
            clothing_save(
                app.clone(),
                owned,
                head.clone().unwrap_or_else(|| d.current_head.clone()),
                voice.clone().unwrap_or_else(|| d.current_voice.clone()),
            )
            .await?;
            Ok(())
        }
        AiOp::ArsenalApply { set_id, weapon_only } => {
            crate::arsenal::arsenal_apply(app.clone(), set_id.clone(), *weapon_only).await?;
            Ok(())
        }
    }
}

/// 货币调整到目标值：现有堆调减/调增，不够的量并入首堆（目标恒正）
fn set_money(app: &AppHandle, tpl: &str, target: i64) -> Result<(), String> {
    let id = profile_id(app)?;
    let dir = profiles_dir(app).ok_or("未选择 SPT 服务器")?;
    let path = std::path::Path::new(&dir).join(format!("{}.json", id));
    let mut raw: Value =
        serde_json::from_str(&fs::read_to_string(&path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    crate::backup::backup_before_write(app, &path)?;

    let items = raw
        .pointer_mut("/characters/pmc/Inventory/items")
        .and_then(|v| v.as_array_mut())
        .ok_or("Inventory.items 缺失")?;
    // 该货币的堆（含在容器内的），先聚合看总量
    let mut money_idx: Vec<usize> = Vec::new();
    for (i, it) in items.iter().enumerate() {
        if it.get("_tpl").and_then(|v| v.as_str()) == Some(tpl) {
            money_idx.push(i);
        }
    }
    if money_idx.is_empty() {
        return Err("仓库里没有该货币堆（打开游戏拾取一次后再试）".into());
    }
    // 首堆设为目标，其余堆清零（0 堆删除，保持库存干净）
    let first = money_idx[0];
    let obj = items[first].as_object_mut().ok_or("货币堆结构异常")?;
    obj.entry("upd").or_insert_with(|| json!({}))
        .as_object_mut().ok_or("货币堆结构异常")?
        .insert("StackObjectsCount".into(), json!(target));
    for &i in &money_idx[1..] {
        items[i] = json!({ "__drop__": true });
    }
    let mut cleaned = 0;
    items.retain(|it| {
        if it.get("__drop__").is_some() {
            cleaned += 1;
            false
        } else {
            true
        }
    });
    let _ = cleaned;
    let text = serde_json::to_string_pretty(&raw).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AI 计划里 arsenalApply 的字段是 camelCase（前端与提示词 schema 都按 `setId`/`weaponOnly`），
    /// enum 必须 rename_all_fields，否则报 `missing field 'set_id'`。
    #[test]
    fn arsenal_apply_op_accepts_camel_case_fields() {
        let op: AiOp = serde_json::from_str(
            r#"{"kind":"arsenalApply","setId":"mid","weaponOnly":true}"#,
        )
        .expect("camelCase setId/weaponOnly 应能反序列化");
        match op {
            AiOp::ArsenalApply { set_id, weapon_only } => {
                assert_eq!(set_id, "mid");
                assert!(weapon_only);
            }
            _ => panic!("变体不符"),
        }
    }

    /// 单词字段的变体不受影响；未知 kind 仍应报错
    #[test]
    fn other_ai_ops_still_deserialize() {
        for raw in [
            r#"{"kind":"skillsAll","level":51}"#,
            r#"{"kind":"moneySet","rub":1000}"#,
            r#"{"kind":"clothingUnlockAll","head":"h","voice":"v"}"#,
            r#"{"kind":"setLook"}"#,
        ] {
            let op: AiOp = serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{raw} 失败: {e}"));
            let _ = op;
        }
        assert!(serde_json::from_str::<AiOp>(r#"{"kind":"nope"}"#).is_err());
    }
}