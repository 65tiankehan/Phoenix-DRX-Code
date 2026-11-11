//! 藏身处页（08-hideout）数据服务 —— 读全量 / 写回（仅 PMC）。
//! 结构对照参考编辑器（HideoutArea.cs / CharacterHideout.cs / StartedHideoutProduction.cs
//! / UnlockedInfo.cs / AppData.LoadHideout*）：
//! - 档案侧：characters.pmc.Hideout.Areas（{type, level} 数组）与
//!   Hideout.Production（recipeId → {recipeId, progress, productionTime, startTimestamp}
//!   字典，时长单位小时）；配方解锁清单 = characters.pmc.UnlockedInfo.unlockedProductionRecipe
//!   （稿图注 1 的 HideoutProductions 为口误，参考实证 UnlockedInfo）；
//! - 服务器侧：hideout/areas.json（[{_id, parentArea, type, stages}] 顶层数组，
//!   MaxLevel = stages 数 − 1）与 hideout/production.json（{recipes: [...]} 包装，
//!   配方行 {_id, areaType, endProduct, locked, ...}）；
//! - 名称：locale 直取 `hideout_area_{type}_name`（缺失原样键）与 `{endProduct} Name`
//!   （缺失原样模板 id）；
//! - 联动口径（参考 HideoutArea.Level setter 实证）：父子同值联动是**前端 UI 操作语义**
//!   （每跳按目标自己的 MaxLevel 卡控、递归传导）；写入侧不做联动，仅按 stages 数卡控，
//!   防止联动把等级顶超各区域自己的上限（如 Solar Power max=1 被父区域顶到 2）；
//! - SetFinished（参考 StartedHideoutProduction.SetFinished）：剩余小时 × 3600 折回
//!   startTimestamp 后 progress 置满（前端算好传回，Rust 原样写）；
//! - 写回铁律：先备份，后整体替换三段数据（Areas / Production / unlockedProductionRecipe）。

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::AppHandle;

use crate::backup::backup_before_write;
use crate::character::database_dir;
use crate::profile::{is_safe_id, profiles_dir, SaveResult};

// ———————— 类型 ————————

/// 区域行（name 已本地化；maxLevel = stages 数 − 1；parentType 供前端父子联动）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HideoutAreaRow {
    #[serde(rename = "type")]
    pub area_type: i64,
    pub level: i64,
    pub max_level: i64,
    pub name: String,
    pub parent_type: Option<i64>,
}

/// 配方行（name = 最终产品本地化名；added 对应存档解锁清单）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HideoutRecipeRow {
    pub id: String,
    pub name: String,
    /// 最终产品模板 id（卡内小字对照）
    pub product_tpl: String,
    pub area: String,
    pub added: bool,
}

/// 进行中生产行（finished = progress ≥ productionTime，参考 IsFinished）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HideoutCraftRow {
    pub id: String,
    pub name: String,
    pub product_tpl: String,
    pub area: String,
    pub finished: bool,
    pub progress: f64,
    pub production_time: f64,
    pub start_timestamp: i64,
}

/// 藏身处页全量数据（仅 PMC；Scav 无藏身处）
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HideoutData {
    pub areas: Vec<HideoutAreaRow>,
    pub recipes: Vec<HideoutRecipeRow>,
    pub crafts: Vec<HideoutCraftRow>,
}

/// 保存载荷：区域等级（全量）+ 解锁清单（全量）+ 进行中生产（全量替换）
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HideoutSaveData {
    pub areas: Vec<HideoutAreaSet>,
    pub unlocked: Vec<String>,
    pub crafts: Vec<HideoutCraftSet>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HideoutAreaSet {
    #[serde(rename = "type")]
    pub area_type: i64,
    pub level: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HideoutCraftSet {
    pub recipe_id: String,
    pub progress: f64,
    pub production_time: f64,
    pub start_timestamp: i64,
}

// ———————— 读侧 helpers ————————

/// 服务器 hideout/areas.json：4.0 及更早是顶层数组，4.1 起是 {areaId: {_id, parentArea, type, stages}} 字典。
/// 两种形态都接受，统一成 Vec<Value>，并保证每项都有 _id。
fn load_area_infos(app: &AppHandle) -> Vec<Value> {
    let Some(db) = database_dir(app) else {
        return Vec::new();
    };
    let Ok(txt) = fs::read_to_string(Path::new(&db).join("hideout").join("areas.json")) else {
        return Vec::new();
    };
    parse_area_infos(&txt)
}

/// areas.json 文本 → 区域信息数组。4.0 及更早是顶层数组，4.1 起是
/// `{areaId: {_id, parentArea, type, stages}}` 字典；两种都接受，并保证每项都有 `_id`。
pub(crate) fn parse_area_infos(txt: &str) -> Vec<Value> {
    match serde_json::from_str::<Value>(txt) {
        Ok(Value::Array(items)) => items,
        Ok(Value::Object(map)) => map
            .into_iter()
            .map(|(id, mut area)| {
                if area.is_object() && area.get("_id").is_none() {
                    area["_id"] = Value::String(id);
                }
                area
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// 服务器 hideout/production.json：{ recipes: [...] } 包装（参考 LoadHideoutProduction）
fn load_recipes(app: &AppHandle) -> Vec<Value> {
    let Some(db) = database_dir(app) else {
        return Vec::new();
    };
    let Ok(txt) = fs::read_to_string(Path::new(&db).join("hideout").join("production.json"))
    else {
        return Vec::new();
    };
    let Ok(v) = serde_json::from_str::<Value>(&txt) else {
        return Vec::new();
    };
    v.get("recipes")
        .and_then(|r| r.as_array().cloned())
        .unwrap_or_default()
}

/// 档案语言（/info/language，缺失回退 en）对应的本地化表
fn locales_table(app: &AppHandle, lang: &str) -> Option<Value> {
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

/// 区域名：locale `hideout_area_{type}_name`，缺失原样键（参考 LocalizedName 兜底）
fn area_name(names: Option<&Value>, area_type: i64) -> String {
    let key = format!("hideout_area_{}_name", area_type);
    names
        .and_then(|n| n.get(&key))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or(key)
}

/// 产品名：locale `{tpl} Name`，缺失原样模板 id（参考 ExaminedItem 兜底）
fn product_name(names: Option<&Value>, tpl: &str) -> String {
    names
        .and_then(|n| n.get(format!("{} Name", tpl)))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| tpl.to_string())
}

// ———————— 命令实现 ————————

/// 藏身处页全量数据（仅 PMC）；档案缺失或不可读时为 None
pub fn load_hideout(app: &AppHandle, id: &str) -> Option<HideoutData> {
    if !is_safe_id(id) {
        return None;
    }
    let dir = profiles_dir(app)?;
    let raw: Value =
        serde_json::from_str(&fs::read_to_string(Path::new(&dir).join(format!("{}.json", id))).ok()?)
            .ok()?;
    let pmc = raw.pointer("/characters/pmc");

    let lang = crate::server::data_language(app, Some(&raw));
    let names = locales_table(app, &lang);

    // 区域：档案 Areas 逐行 + areas.json 的 stages 数定上限
    let area_infos = load_area_infos(app);
    let max_of = |t: i64| -> i64 {
        area_infos
            .iter()
            .find(|a| a.get("type").and_then(|v| v.as_i64()) == Some(t))
            .and_then(|a| a.get("stages"))
            .and_then(|s| s.as_object())
            .map(|o| o.len() as i64 - 1)
            .unwrap_or(0)
    };
    let areas: Vec<HideoutAreaRow> = pmc
        .and_then(|p| p.pointer("/Hideout/Areas"))
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|row| {
                    let t = row.get("type").and_then(|v| v.as_i64())?;
                    // 父区域 type：areas.json 本区域 parentArea id → 父信息行的 type
                    let parent_type = area_infos
                        .iter()
                        .find(|a| a.get("type").and_then(|v| v.as_i64()) == Some(t))
                        .and_then(|a| a.get("parentArea").and_then(|p| p.as_str()))
                        .and_then(|pid| {
                            area_infos
                                .iter()
                                .find(|a| a.get("_id").and_then(|v| v.as_str()) == Some(pid))
                                .and_then(|a| a.get("type").and_then(|v| v.as_i64()))
                        });
                    Some(HideoutAreaRow {
                        area_type: t,
                        level: row.get("level").and_then(|v| v.as_f64()).unwrap_or(0.0)
                            as i64,
                        max_level: max_of(t),
                        name: area_name(names.as_ref(), t),
                        parent_type,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    // 配方：production.json 逐行 + 解锁清单（UnlockedInfo.unlockedProductionRecipe）
    let recipes = load_recipes(app);
    let unlocked: Vec<String> = pmc
        .and_then(|p| p.pointer("/UnlockedInfo/unlockedProductionRecipe"))
        .and_then(|u| u.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let recipe_rows: Vec<HideoutRecipeRow> = recipes
        .iter()
        .filter_map(|p| {
            let rid = p.get("_id").and_then(|v| v.as_str())?;
            let area_type = p.get("areaType").and_then(|v| v.as_i64()).unwrap_or(0);
            let product_tpl = p.get("endProduct").and_then(|v| v.as_str()).unwrap_or("");
            Some(HideoutRecipeRow {
                id: rid.to_string(),
                name: product_name(names.as_ref(), product_tpl),
                product_tpl: product_tpl.to_string(),
                area: area_name(names.as_ref(), area_type),
                added: unlocked.iter().any(|u| u == rid),
            })
        })
        .collect();

    // 进行中生产：档案 Production 字典 + production.json 回填产品 / 区域名
    let recipe_meta: HashMap<&str, (&str, i64)> = recipes
        .iter()
        .filter_map(|p| {
            Some((
                p.get("_id")?.as_str()?,
                (
                    p.get("endProduct")?.as_str().unwrap_or(""),
                    p.get("areaType").and_then(|v| v.as_i64()).unwrap_or(0),
                ),
            ))
        })
        .collect();
    let crafts: Vec<HideoutCraftRow> = pmc
        .and_then(|p| p.pointer("/Hideout/Production"))
        .and_then(|d| d.as_object())
        .map(|obj| {
            obj.values()
                .filter_map(|c| {
                    let rid = c.get("recipeId").and_then(|v| v.as_str())?;
                    let progress = c.get("progress").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let ptime = c
                        .get("productionTime")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    let (tpl, area_type) =
                        recipe_meta.get(rid).copied().unwrap_or(("", 0));
                    let finished = ptime > 0.0 && progress >= ptime;
                    Some(HideoutCraftRow {
                        id: rid.to_string(),
                        name: if tpl.is_empty() {
                            rid.to_string()
                        } else {
                            product_name(names.as_ref(), tpl)
                        },
                        product_tpl: tpl.to_string(),
                        area: if tpl.is_empty() {
                            "unknown".to_string()
                        } else {
                            area_name(names.as_ref(), area_type)
                        },
                        finished,
                        progress,
                        production_time: ptime,
                        start_timestamp: c
                            .get("startTimestamp")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Some(HideoutData {
        areas,
        recipes: recipe_rows,
        crafts,
    })
}

// ———————— 写回 ————————

/// 藏身处页写回：先强制备份（领域铁律），后整体替换三段数据
pub fn save_hideout(app: &AppHandle, id: &str, data: &HideoutSaveData) -> Result<SaveResult, String> {
    if !is_safe_id(id) {
        return Err(format!("Invalid profile id: {}", id));
    }
    let dir = profiles_dir(app).ok_or_else(|| "未选择 SPT 服务器".to_string())?;
    let path = Path::new(&dir).join(format!("{}.json", id));
    let txt = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut raw: Value = serde_json::from_str(&txt).map_err(|e| e.to_string())?;
    let backup_path = backup_before_write(app, &path)?;

    let area_infos = load_area_infos(app);
    // 上限表：stages 数 − 1
    let mut maxes: HashMap<i64, i64> = HashMap::new();
    for a in &area_infos {
        if let (Some(t), Some(s)) = (
            a.get("type").and_then(|v| v.as_i64()),
            a.get("stages").and_then(|s| s.as_object()),
        ) {
            maxes.insert(t, s.len() as i64 - 1);
        }
    }
    let levels: HashMap<i64, i64> = data
        .areas
        .iter()
        .map(|a| {
            let lv = a.level.max(0);
            // 数据库未收录的区域（4.1.6 存档里残留的 type 21 等）不设上限，
            // 否则一次保存就会被压成 0 级（2026-11-11 事故的第二半）
            let capped = match maxes.get(&a.area_type) {
                Some(cap) => lv.min(*cap),
                None => lv,
            };
            (a.area_type, capped)
        })
        .collect();

    // Hideout 节缺失则建
    if raw.pointer("/characters/pmc/Hideout").is_none() {
        if let Some(pmc) = raw.pointer_mut("/characters/pmc") {
            pmc["Hideout"] = json!({ "Areas": [], "Production": {} });
        }
    }

    // Areas：按 type 合并重建 —— 只改 level，其余字段逐字保留（见 merge_areas 注释）
    let original_areas: Vec<Value> = raw
        .pointer("/characters/pmc/Hideout/Areas")
        .and_then(|a| a.as_array())
        .cloned()
        .unwrap_or_default();
    let areas_out = merge_areas(&original_areas, &levels);
    if let Some(h) = raw.pointer_mut("/characters/pmc/Hideout") {
        h["Areas"] = Value::Array(areas_out);

        // Production：全量替换（键 = recipeId；完成折算已由前端算好）
        let mut prod = serde_json::Map::new();
        for c in &data.crafts {
            prod.insert(
                c.recipe_id.clone(),
                json!({
                    "recipeId": c.recipe_id,
                    "progress": c.progress,
                    "productionTime": c.production_time,
                    "startTimestamp": c.start_timestamp
                }),
            );
        }
        h["Production"] = Value::Object(prod);
    }

    // 解锁清单：UnlockedInfo 缺失则建；去重保序
    let mut unlocked: Vec<Value> = Vec::new();
    for u in &data.unlocked {
        if !u.is_empty() && !unlocked.iter().any(|x| x.as_str() == Some(u)) {
            unlocked.push(json!(u));
        }
    }
    if let Some(pmc) = raw.pointer_mut("/characters/pmc") {
        if pmc.get("UnlockedInfo").is_none() {
            pmc["UnlockedInfo"] = json!({});
        }
        if let Some(ui) = pmc.get_mut("UnlockedInfo") {
            ui["unlockedProductionRecipe"] = Value::Array(unlocked);
        }
    }

    let out = serde_json::to_string_pretty(&raw).map_err(|e| e.to_string())?;
    fs::write(&path, out).map_err(|e| e.to_string())?;
    Ok(SaveResult {
        backup_path: backup_path.map(|p| p.to_string_lossy().to_string()),
    })
}

// ———————— 写入侧纯函数（可单测） ————————

/// 把「区域等级表」合并进原 Areas 数组：只改 `level`，其余字段逐字保留。
///
/// 参考 4.1.1 `HideoutArea.Level` setter 只动 Level（其余字段由强类型模型天然保留）；
/// 本项目直接操作裸 JSON，必须显式保住 `active / passiveBonusesEnabled / completeTime /
/// constructing / slots / lastRecipe`。
///
/// 2026-11-11 事故：旧实现只写 `{type, level}`，把 `slots` 与 `active` 一起清空；叠加
/// `areas.json` 顶层字典被当成数组读不到（cap 恒为 0），存档里 Stash 从 4 级被压成 0 级，
/// 而仓库物品仍在 → 客户端读档后卡死在 EFT 启动画面。
pub(crate) fn merge_areas(original: &[Value], levels: &HashMap<i64, i64>) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();

    for (t, lv) in levels {
        let merged = original
            .iter()
            .find(|row| row.get("type").and_then(|v| v.as_i64()) == Some(*t))
            .filter(|row| row.is_object())
            .cloned()
            .map(|mut row| {
                row["level"] = json!(lv);
                // 有等级必然已建造激活：原值为 false 时补正，避免「已升级但未激活」的非法组合
                if *lv > 0 && row.get("active").and_then(|v| v.as_bool()) != Some(true) {
                    row["active"] = json!(true);
                }
                row
            })
            .unwrap_or_else(|| {
                // 存档里还没有这个区域：按 4.1.6 服务器序列化形态补齐全套字段
                json!({
                    "type": t,
                    "level": lv,
                    "active": *lv > 0,
                    "passiveBonusesEnabled": true,
                    "completeTime": 0,
                    "constructing": false,
                    "slots": [],
                    "lastRecipe": ""
                })
            });
        out.push(merged);
    }

    // 数据库里已不存在、但存档里仍留着的区域（例如 4.1.6 的 type 21）：原样保留，不擅自删除
    for row in original {
        let t = row.get("type").and_then(|v| v.as_i64());
        if !out
            .iter()
            .any(|a| a.get("type").and_then(|v| v.as_i64()) == t)
        {
            out.push(row.clone());
        }
    }

    out.sort_by_key(|a| a.get("type").and_then(|v| v.as_i64()).unwrap_or(0));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_areas() -> Vec<Value> {
        vec![
            json!({
                "type": 3, "level": 4, "active": true, "passiveBonusesEnabled": true,
                "completeTime": 0, "constructing": false, "slots": [],
                "lastRecipe": ""
            }),
            json!({
                "type": 1, "level": 0, "active": true, "passiveBonusesEnabled": true,
                "completeTime": 0, "constructing": false, "slots": [],
                "lastRecipe": ""
            }),
            json!({
                "type": 20, "level": 0, "active": true, "passiveBonusesEnabled": true,
                "completeTime": 0, "constructing": false,
                "slots": [{"locationIndex": 0}, {"locationIndex": 1}],
                "lastRecipe": ""
            }),
        ]
    }

    #[test]
    fn bump_keeps_every_other_field_of_that_area() {
        let original = sample_areas();
        let mut levels = HashMap::new();
        levels.insert(3, 4);
        levels.insert(1, 1); // 安保室 0 → 1（用户报告的操作）
        levels.insert(20, 0);

        let out = merge_areas(&original, &levels);
        assert_eq!(out.len(), 3);

        let sec = out
            .iter()
            .find(|a| a.get("type").and_then(|v| v.as_i64()) == Some(1))
            .unwrap();
        assert_eq!(sec.get("level").and_then(|v| v.as_i64()), Some(1));
        assert_eq!(sec.get("active").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(
            sec.get("passiveBonusesEnabled").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(sec.get("completeTime").and_then(|v| v.as_i64()), Some(0));
        assert_eq!(
            sec.get("constructing").and_then(|v| v.as_bool()),
            Some(false)
        );
        assert_eq!(sec.get("slots").and_then(|v| v.as_array()).map(|a| a.len()), Some(0));

        // 有槽位的区域，槽位必须原样保留
        let stash = out
            .iter()
            .find(|a| a.get("type").and_then(|v| v.as_i64()) == Some(20))
            .unwrap();
        assert_eq!(
            stash.get("slots").and_then(|v| v.as_array()).map(|a| a.len()),
            Some(2)
        );
    }

    #[test]
    fn unknown_type_in_profile_is_not_dropped() {
        let mut original = sample_areas();
        original.push(json!({"type": 99, "level": 0, "active": true}));
        let mut levels = HashMap::new();
        levels.insert(1, 1);

        let out = merge_areas(&original, &levels);
        assert!(
            out.iter()
                .any(|a| a.get("type").and_then(|v| v.as_i64()) == Some(99)),
            "存档里有、数据库里没有的区域不能被删掉"
        );
    }

    #[test]
    fn missing_area_is_created_with_full_field_set() {
        let original: Vec<Value> = Vec::new();
        let mut levels = HashMap::new();
        levels.insert(6, 2);

        let out = merge_areas(&original, &levels);
        assert_eq!(out.len(), 1);
        let row = &out[0];
        for key in [
            "type",
            "level",
            "active",
            "passiveBonusesEnabled",
            "completeTime",
            "constructing",
            "slots",
            "lastRecipe",
        ] {
            assert!(row.get(key).is_some(), "新建区域缺少字段 {}", key);
        }
        assert_eq!(row.get("active").and_then(|v| v.as_bool()), Some(true));
    }

    #[test]
    fn areas_json_dict_form_is_accepted() {
        // 4.1.6 的真实形态：顶层字典，键是 areaId
        let txt = r#"{
            "63db64cbf9963741dc0d741f": {"type": 25, "stages": {"0": {}, "1": {}}, "parentArea": ""},
            "5d78a7d8ea1e0a0f8c1a4b3c": {"_id": "5d78a7d8ea1e0a0f8c1a4b3c", "type": 1,
                "stages": {"0": {}, "1": {}, "2": {}, "3": {}}}
        }"#;
        let infos = parse_area_infos(txt);
        assert_eq!(infos.len(), 2);
        let security = infos
            .iter()
            .find(|a| a.get("type").and_then(|v| v.as_i64()) == Some(1))
            .unwrap();
        assert_eq!(
            security.get("stages").and_then(|s| s.as_object()).map(|o| o.len()),
            Some(4),
            "stages 数决定上限，读到才能算出 3 级"
        );
        let child = infos
            .iter()
            .find(|a| a.get("type").and_then(|v| v.as_i64()) == Some(25))
            .unwrap();
        assert_eq!(
            child.get("_id").and_then(|v| v.as_str()),
            Some("63db64cbf9963741dc0d741f"),
            "字典形态要把键补成 _id"
        );
    }

    #[test]
    fn areas_json_array_form_still_works() {
        let txt = r#"[{"_id": "abc", "type": 3, "stages": {"0": {}, "1": {}, "2": {}, "3": {}, "4": {}}}]"#;
        let infos = parse_area_infos(txt);
        assert_eq!(infos.len(), 1);
        assert_eq!(
            infos[0].get("stages").and_then(|s| s.as_object()).map(|o| o.len()),
            Some(5)
        );
    }

    #[test]
    fn capped_level_zeroes_are_not_forced_when_cap_unknown() {
        // 数据库未收录的区域不能被压成 0（cap 缺失时保持原值）
        let rows = vec![
            HideoutAreaSet { area_type: 1, level: 2 },
            HideoutAreaSet { area_type: 21, level: 1 },
        ];
        let mut maxes: HashMap<i64, i64> = HashMap::new();
        maxes.insert(1, 3);
        let levels: HashMap<i64, i64> = rows
            .iter()
            .map(|a| {
                let lv = a.level.max(0);
                let capped = match maxes.get(&a.area_type) {
                    Some(cap) => lv.min(*cap),
                    None => lv,
                };
                (a.area_type, capped)
            })
            .collect();
        assert_eq!(levels.get(&1), Some(&2));
        assert_eq!(levels.get(&21), Some(&1), "未收录区域不该被压成 0");
    }

    /// 真机回归：拿本机真实存档跑一遍「安保室 0 → 1」，除该条 level 外整块必须逐字不变。
    #[test]
    #[ignore = "需要本机真实存档 E:\\EFT\\SPT_Runtime\\user\\profiles"]
    fn real_profile_areas_only_change_requested_level() {
        let dir = r"E:\EFT\SPT_Runtime\user\profiles";
        let Some(entry) = fs::read_dir(dir)
            .ok()
            .and_then(|it| it.filter_map(|e| e.ok()).find(|e| e.path().extension().is_some_and(|x| x == "json")))
        else {
            return;
        };
        let txt = fs::read_to_string(entry.path()).unwrap();
        let raw: Value = serde_json::from_str(&txt).unwrap();
        let original: Vec<Value> = raw
            .pointer("/characters/pmc/Hideout/Areas")
            .and_then(|a| a.as_array())
            .cloned()
            .expect("真实存档应有 Hideout.Areas");

        let mut levels: HashMap<i64, i64> = HashMap::new();
        for row in &original {
            let t = row.get("type").and_then(|v| v.as_i64()).unwrap();
            let lv = row.get("level").and_then(|v| v.as_f64()).unwrap_or(0.0) as i64;
            levels.insert(t, lv);
        }
        levels.insert(1, 1); // 安保室 +1

        let out = merge_areas(&original, &levels);
        assert_eq!(out.len(), original.len(), "区域条目数不能变");

        for before in &original {
            let t = before.get("type").and_then(|v| v.as_i64()).unwrap();
            let after = out
                .iter()
                .find(|a| a.get("type").and_then(|v| v.as_i64()) == Some(t))
                .unwrap();
            let want = if t == 1 {
                1
            } else {
                before.get("level").and_then(|v| v.as_f64()).unwrap_or(0.0) as i64
            };
            assert_eq!(
                after.get("level").and_then(|v| v.as_i64()),
                Some(want),
                "type={} 的等级不对",
                t
            );
            for (k, v) in before.as_object().unwrap() {
                if k == "level" {
                    continue;
                }
                assert_eq!(after.get(k), Some(v), "type={} 的字段 {} 被改动了", t, k);
            }
        }
    }
}
