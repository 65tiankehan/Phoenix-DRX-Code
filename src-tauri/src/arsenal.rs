//! 套装军械库（稿图 32，0.2.0 第 3 段）——预设页「套装军械库」tab 的数据层。
//! 套餐清单配置驱动（src-tauri/data/arsenal-sets.json，include_str! 内嵌），运行时对真实服务器
//! 数据库解析：武器按 globals.json ItemPresets 的 _name 匹配候选；防具按 templates/items.json
//! 的 ShortName 匹配候选。找不到的件标「未找到」并跳过（诚实原则，绝不编造模板 id）。
//! 入库：武器走 stash::add_build_to_stash（复用预设页通路），防具走放置引擎 + 满耐久 upd.Repairable；
//! 写入前一律 backup_before_write（章程 §四 铁律）。价格仅展示，入库不扣钱（稿图 32 口径）。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use tauri::AppHandle;

use crate::backup::backup_before_write;
use crate::character::database_dir;
use crate::profile::{get_current_profile_id, profiles_dir};
use crate::stash::{add_build_to_stash, find_free_slot, ItemsDb};

const SETS_JSON: &str = include_str!("../data/arsenal-sets.json");

#[derive(Deserialize)]
struct SetDef {
    id: String,
    name: String,
    #[serde(default)]
    tier: String,
    #[serde(default)]
    desc: String,
    #[serde(default)]
    price_rub: i64,
    #[serde(default)]
    weapon: Option<WeaponDef>,
    #[serde(default)]
    armor: Vec<PieceDef>,
}

#[derive(Deserialize, Clone)]
struct WeaponDef {
    candidates: Vec<String>,
}

#[derive(Deserialize)]
struct PieceDef {
    kind: String,
    candidates: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArsenalPiece {
    kind: String,
    /// 解析出的展示名（ShortName 命中或预设名）；未找到 = None
    name: Option<String>,
    detail: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArsenalSet {
    pub(crate) id: String,
    pub(crate) name: String,
    tier: String,
    desc: String,
    price_rub: i64,
    weapon: Option<ArsenalPiece>,
    pieces: Vec<ArsenalPiece>,
    /// 全部条目都解析成功才为 true（按钮可用性依据）
    resolvable: bool,
}

fn eq_ci(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// 解析上下文：items.json 原始表（ShortName/_id/MaxDurability）+ ItemPresets
struct Ctx {
    items: HashMap<String, Value>, // tpl -> entry
    short_index: HashMap<String, String>, // lower(shortname) -> tpl
    presets: Vec<(String, String)>, // (preset_id, _name)
}

fn load_ctx(app: &AppHandle) -> Result<Ctx, String> {
    let db = database_dir(app).ok_or("未选择 SPT 服务器")?;
    let mut items = HashMap::new();
    let mut short_index = HashMap::new();
    if let Ok(txt) = fs::read_to_string(std::path::Path::new(&db).join("templates").join("items.json")) {
        if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&txt) {
            for (tpl, entry) in map {
                let short = entry
                    .pointer("/_props/ShortName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if !short.is_empty() {
                    short_index.insert(short.to_lowercase(), tpl.clone());
                }
                items.insert(tpl, entry);
            }
        }
    }
    let mut presets = Vec::new();
    if let Ok(txt) = fs::read_to_string(std::path::Path::new(&db).join("globals.json")) {
        if let Ok(v) = serde_json::from_str::<Value>(&txt) {
            if let Some(map) = v.pointer("/config/ItemPresets").and_then(|p| p.as_object()) {
                for (pid, p) in map {
                    let name = p.get("_name").and_then(|n| n.as_str()).unwrap_or("");
                    if !name.is_empty() {
                        presets.push((pid.clone(), name.to_string()));
                    }
                }
            }
        }
    }
    Ok(Ctx { items, short_index, presets })
}

fn resolve_weapon(ctx: &Ctx, def: &Option<WeaponDef>) -> Option<(String, String)> {
    let w = def.as_ref()?;
    for c in &w.candidates {
        for (pid, name) in &ctx.presets {
            if eq_ci(name, c) {
                return Some((pid.clone(), name.clone()));
            }
        }
    }
    None
}

fn resolve_piece(ctx: &Ctx, p: &PieceDef) -> Option<(String, String)> {
    for c in &p.candidates {
        if let Some(tpl) = ctx.short_index.get(&c.to_lowercase()) {
            let name = ctx
                .items
                .get(tpl)
                .and_then(|e| e.pointer("/_props/ShortName"))
                .and_then(|v| v.as_str())
                .unwrap_or(c);
            return Some((tpl.clone(), name.to_string()));
        }
    }
    None
}

fn piece_view(kind: &str, resolved: Option<(String, String)>, candidates: &[String]) -> ArsenalPiece {
    match resolved {
        Some((_, name)) => ArsenalPiece { kind: kind.into(), name: Some(name), detail: String::new() },
        None => ArsenalPiece {
            kind: kind.into(),
            name: None,
            detail: candidates.first().cloned().unwrap_or_default(),
        },
    }
}

/// 读取 12 套套餐（含对真实库的解析结果；前端渲染左墙右详）
#[tauri::command]
pub async fn arsenal_load(app: AppHandle) -> Result<Vec<ArsenalSet>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let ctx = load_ctx(&app)?;
        let defs: Value = serde_json::from_str(SETS_JSON).map_err(|e| e.to_string())?;
        let raw_sets = defs
            .get("sets")
            .and_then(|v| v.as_array())
            .ok_or("arsenal-sets.json 缺 sets")?
            .clone();
        let mut out = Vec::with_capacity(raw_sets.len());
        for rs in raw_sets {
            let Ok(def) = serde_json::from_value::<SetDef>(rs) else {
                continue;
            };
            let w_res = resolve_weapon(&ctx, &def.weapon);
            let mut pieces = Vec::new();
            for p in &def.armor {
                let r = resolve_piece(&ctx, p);
                pieces.push(piece_view(&p.kind, r, &p.candidates));
            }
            let resolvable = (def.weapon.is_none() || w_res.is_some())
                && pieces.iter().all(|p| p.name.is_some());
            out.push(ArsenalSet {
                weapon: def.weapon.as_ref().map(|w| piece_view("weapon", w_res.clone().map(|(a, b)| (a, b)), &w.candidates)),
                id: def.id,
                name: def.name,
                tier: def.tier,
                desc: def.desc,
                price_rub: def.price_rub,
                pieces,
                resolvable,
            });
        }
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn profile_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let id = get_current_profile_id(app).ok_or("未选择存档")?;
    let dir = profiles_dir(app).ok_or("未选择 SPT 服务器")?;
    Ok(std::path::Path::new(&dir).join(format!("{}.json", id)))
}

/// 应用套装入库（weapon_only=true 只入武器）。走备份链；满耐久写 upd.Repairable。
#[tauri::command]
pub async fn arsenal_apply(
    app: AppHandle,
    set_id: String,
    weapon_only: bool,
) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let ctx = load_ctx(&app)?;
        let defs: Value = serde_json::from_str(SETS_JSON).map_err(|e| e.to_string())?;
        let raw_sets = defs
            .get("sets")
            .and_then(|v| v.as_array())
            .ok_or("arsenal-sets.json 缺 sets")?;
        let def = raw_sets
            .iter()
            .filter_map(|rs| serde_json::from_value::<SetDef>(rs.clone()).ok())
            .find(|d| d.id == set_id)
            .ok_or_else(|| format!("查无该套装：{}", set_id))?;

        let path = profile_path(&app)?;
        let mut raw: Value =
            serde_json::from_str(&fs::read_to_string(&path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        let backup = backup_before_write(&app, &path)?;

        let stash_id = raw
            .pointer("/characters/pmc/Inventory/stash")
            .and_then(|v| v.as_str())
            .ok_or("存档缺 stash 容器")?
            .to_string();
        let db_dir = database_dir(&app).ok_or("未选择 SPT 服务器")?;
        let db = ItemsDb::load(&app, None);
        // 与 stash.rs new_item_id 同源的序号（取 inventory 现有最大短 id 递增，避免撞号）
        let seq = raw
            .pointer("/characters/pmc/Inventory/items")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|it| it.get("_id").and_then(|v| v.as_str()))
                    .filter_map(|s| s.parse::<u32>().ok())
                    .max()
                    .unwrap_or(0)
                    + 1
            })
            .unwrap_or(1);

        if let Some(w) = &def.weapon {
            if let Some((pid, _)) = resolve_weapon(&ctx, &Some((*w).clone())) {
                add_build_to_stash(&mut raw, &stash_id, &pid, &db_dir, &db, seq)?;
            }
        }
        if !weapon_only {
            let mut n = seq + 1;
            for p in &def.armor {
                let Some((tpl, _)) = resolve_piece(&ctx, p) else { continue };
                add_gear(&mut raw, &stash_id, &tpl, &db_dir, &db, &ctx, n)?;
                n += 1;
            }
        }

        let text = serde_json::to_string_pretty(&raw).map_err(|e| e.to_string())?;
        fs::write(&path, text).map_err(|e| e.to_string())?;
        Ok(backup.map(|p| p.to_string_lossy().to_string()))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 防具入库：放置引擎找空位 + upd.Repairable 满耐久（MaxDurability 取自模板，缺省不写）
fn add_gear(
    raw: &mut Value,
    stash_id: &str,
    tpl: &str,
    db_dir: &str,
    db: &ItemsDb,
    ctx: &Ctx,
    seq: u32,
) -> Result<(), String> {
    let (x, y, rotated) = find_free_slot(raw, stash_id, tpl, db_dir, db)?;
    let items = raw
        .pointer_mut("/characters/pmc/Inventory/items")
        .and_then(|v| v.as_array_mut())
        .ok_or("Inventory.items 缺失")?;
    let stack_max = db.entries.get(tpl).map(|p| p.stack_max).unwrap_or(1);
    let max_dur = ctx
        .items
        .get(tpl)
        .and_then(|e| e.pointer("/_props/MaxDurability"))
        .and_then(|v| v.as_f64());
    let mut upd = json!({});
    if let Some(d) = max_dur {
        upd["Repairable"] = json!({ "Durability": d, "MaxDurability": d });
    }
    if stack_max > 1 {
        upd["StackObjectsCount"] = json!(1);
    }
    let mut it = json!({
        "_id": crate::stash::new_item_id(seq),
        "_tpl": tpl,
        "parentId": stash_id,
        "slotId": "hideout",
        "location": { "x": x, "y": y, "r": if rotated { 1 } else { 0 } }
    });
    if upd.as_object().map(|o| !o.is_empty()).unwrap_or(false) {
        it["upd"] = upd;
    }
    items.push(it);
    Ok(())
}
