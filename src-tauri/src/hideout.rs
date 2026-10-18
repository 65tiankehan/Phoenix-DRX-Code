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
//! - 联动口径（参考 HideoutArea.Level setter）：等级写入按 stages 数卡控后，
//!   父 / 子区域同步同值（parentArea 关系递归传导，写入侧兜底做不动点收敛）；
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

/// 服务器 hideout/areas.json：顶层数组 [{_id, parentArea, type, stages}]
fn load_area_infos(app: &AppHandle) -> Vec<Value> {
    let Some(db) = database_dir(app) else {
        return Vec::new();
    };
    let Ok(txt) = fs::read_to_string(Path::new(&db).join("hideout").join("areas.json")) else {
        return Vec::new();
    };
    serde_json::from_str::<Value>(&txt)
        .ok()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
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

    let lang = raw
        .pointer("/info/language")
        .and_then(|v| v.as_str())
        .unwrap_or("en");
    let names = locales_table(app, lang);

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

/// 区域等级写入侧兜底：按 stages 数卡控 + 父 / 子区域联动
/// （参考 HideoutArea.Level setter：卡控后把同值同步给 parentArea 关系的父与子，
/// 递归传导 → 用不动点收敛近似）。
fn propagate_levels(levels: &mut HashMap<i64, i64>, area_infos: &[Value]) {
    // type → (_id, parentArea)
    let rel: Vec<(i64, Option<&str>, &str)> = area_infos
        .iter()
        .filter_map(|a| {
            Some((
                a.get("type")?.as_i64()?,
                a.get("parentArea").and_then(|v| v.as_str()),
                a.get("_id")?.as_str()?,
            ))
        })
        .collect();
    loop {
        let mut changed = false;
        for (t, _parent, id) in &rel {
            // 父 → 子：parentArea == 本区域 id 的子区域取同值
            for (ct, cp, _) in &rel {
                if *cp == Some(id) {
                    let want = levels.get(t).copied().unwrap_or(0);
                    if levels.get(ct).copied() != Some(want) {
                        levels.insert(*ct, want);
                        changed = true;
                    }
                }
            }
            // 子 → 父：本区域 parentArea 指向的父区域取同值
            if let Some((pt, _, _)) = rel.iter().find(|(_, _, pid)| {
                Some(*pid) == *_parent
            }) {
                let want = levels.get(t).copied().unwrap_or(0);
                if levels.get(pt).copied() != Some(want) {
                    levels.insert(*pt, want);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
}

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
    let mut levels: HashMap<i64, i64> = data
        .areas
        .iter()
        .map(|a| {
            let cap = maxes.get(&a.area_type).copied().unwrap_or(0);
            (a.area_type, a.level.max(0).min(cap))
        })
        .collect();
    propagate_levels(&mut levels, &area_infos);

    // Hideout 节缺失则建
    if raw.pointer("/characters/pmc/Hideout").is_none() {
        if let Some(pmc) = raw.pointer_mut("/characters/pmc") {
            pmc["Hideout"] = json!({ "Areas": [], "Production": {} });
        }
    }

    // Areas：按 type 整体重建（联动后的等级为准）
    let mut areas_out: Vec<Value> = Vec::new();
    for (t, lv) in &levels {
        areas_out.push(json!({ "type": t, "level": lv }));
    }
    areas_out.sort_by_key(|a| a.get("type").and_then(|v| v.as_i64()).unwrap_or(0));
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
