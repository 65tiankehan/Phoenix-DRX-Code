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
#[serde(rename_all = "camelCase")]
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
    /// 首选：globals.ItemPresets 的 preset id（4.1.6 校准后的主通路）
    #[serde(default)]
    preset: Option<String>,
    /// 展示名（缺省回退预设自身的 _name）
    #[serde(default)]
    label: Option<String>,
    /// 兼容旧配置：按预设名匹配（做归一化后比较）
    #[serde(default)]
    candidates: Vec<String>,
}

#[derive(Deserialize)]
struct PieceDef {
    kind: String,
    /// 首选：templates/items.json 的 tpl
    #[serde(default)]
    tpl: Option<String>,
    /// 展示名（缺省回退模板的 ShortName）
    #[serde(default)]
    label: Option<String>,
    /// 兼容旧配置：按 ShortName 匹配
    #[serde(default)]
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

/// 宽松归一化：忽略大小写与 `_ - . 空格` 的差异
/// （`AK-74M_default` 与 `AK-74M Default` 视为同一个名字）
fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, '_' | '-' | '.' | ' '))
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn eq_loose(a: &str, b: &str) -> bool {
    norm(a) == norm(b)
}

/// 解析上下文：items.json 原始表（ShortName/_id/MaxDurability）+ ItemPresets
pub(crate) struct Ctx {
    items: HashMap<String, Value>, // tpl -> entry
    short_index: HashMap<String, String>, // lower(shortname) -> tpl
    presets: Vec<(String, String)>, // (preset_id, _name)
}fn load_ctx(app: &AppHandle) -> Result<Ctx, String> {
    let db = database_dir(app).ok_or("未选择 SPT 服务器")?;
    Ok(ctx_from_db(&db))
}

/// 从 database 目录构建解析上下文（与 AppHandle 解耦，便于真机自检）
pub(crate) fn ctx_from_db(db: &str) -> Ctx {
    let mut items = HashMap::new();
    let mut short_index = HashMap::new();
    if let Ok(txt) = fs::read_to_string(std::path::Path::new(db).join("templates").join("items.json")) {
        if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&txt) {
            for (tpl, entry) in map {
                let short = entry
                    .pointer("/_props/ShortName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if !short.is_empty() {
                    short_index.entry(short.to_lowercase()).or_insert_with(|| tpl.clone());
                }
                items.insert(tpl, entry);
            }
        }
    }
    let mut presets = Vec::new();
    if let Ok(txt) = fs::read_to_string(std::path::Path::new(db).join("globals.json")) {
        if let Ok(v) = serde_json::from_str::<Value>(&txt) {
            if let Some(map) = crate::character::item_presets(&v) {
                for (pid, p) in map {
                    let name = p.get("_name").and_then(|n| n.as_str()).unwrap_or("");
                    if !name.is_empty() {
                        presets.push((pid.clone(), name.to_string()));
                    }
                }
            }
        }
    }
    Ctx { items, short_index, presets }
}

/// 全部套装的解析结果（(set_id, 是否全部解析成功)），供真机自检使用
#[cfg(test)]
pub(crate) fn resolve_all(db: &str) -> Vec<(String, bool)> {
    let ctx = ctx_from_db(db);
    let defs: Value = match serde_json::from_str(SETS_JSON) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let empty = Vec::new();
    let raw_sets = defs.get("sets").and_then(|v| v.as_array()).unwrap_or(&empty);
    let mut out = Vec::new();
    for rs in raw_sets {
        let Ok(def) = serde_json::from_value::<SetDef>(rs.clone()) else {
            continue;
        };
        let w_ok = def.weapon.is_none() || resolve_weapon(&ctx, &def.weapon).is_some();
        let p_ok = def.armor.iter().all(|p| resolve_piece(&ctx, p).is_some());
        out.push((def.id, w_ok && p_ok));
    }
    out
}

fn resolve_weapon(ctx: &Ctx, def: &Option<WeaponDef>) -> Option<(String, String)> {
    let w = def.as_ref()?;
    if let Some(pid) = &w.preset {
        if let Some((_, name)) = ctx.presets.iter().find(|(p, _)| p == pid) {
            return Some((pid.clone(), w.label.clone().unwrap_or_else(|| name.clone())));
        }
    }
    for c in &w.candidates {
        if let Some((pid, name)) = ctx.presets.iter().find(|(_, n)| eq_loose(n, c)) {
            return Some((pid.clone(), name.clone()));
        }
    }
    None
}

fn resolve_piece(ctx: &Ctx, p: &PieceDef) -> Option<(String, String)> {
    if let Some(tpl) = &p.tpl {
        if let Some(entry) = ctx.items.get(tpl) {
            let short = entry
                .pointer("/_props/ShortName")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let shown = p.label.clone().unwrap_or_else(|| short.to_string());
            return Some((tpl.clone(), shown));
        }
    }
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

/// 未解析时的兜底显示：优先配置里的中文 label，其次旧候选名，最后 tpl
fn fallback_label(label: Option<&String>, candidates: &[String], tpl: Option<&String>) -> String {
    if let Some(l) = label {
        return l.clone();
    }
    if let Some(c) = candidates.first() {
        return c.clone();
    }
    tpl.cloned().unwrap_or_default()
}

fn piece_view(kind: &str, resolved: Option<(String, String)>, fallback: String) -> ArsenalPiece {
    match resolved {
        Some((_, name)) => ArsenalPiece { kind: kind.into(), name: Some(name), detail: String::new() },
        None => ArsenalPiece { kind: kind.into(), name: None, detail: fallback },
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
                let fb = fallback_label(p.label.as_ref(), &p.candidates, p.tpl.as_ref());
                pieces.push(piece_view(&p.kind, r, fb));
            }
            let resolvable = (def.weapon.is_none() || w_res.is_some())
                && pieces.iter().all(|p| p.name.is_some());
            out.push(ArsenalSet {
                weapon: def.weapon.as_ref().map(|w| {
                    let fb = fallback_label(w.label.as_ref(), &w.candidates, w.preset.as_ref());
                    piece_view("weapon", w_res.clone(), fb)
                }),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loose_match_ignores_separators() {
        assert!(eq_loose("AK-74M_default", "AK-74M Default"));
        assert!(eq_loose("m4a1_std", "M4A1_Std"));
        assert!(!eq_loose("AK-74M Default", "AK-101 Default"));
    }

    fn ctx(presets: &[(&str, &str)], items: &[(&str, (&str, &str))]) -> Ctx {
        let mut map = HashMap::new();
        let mut idx = HashMap::new();
        for (tpl, (short, name)) in items {
            map.insert(
                tpl.to_string(),
                json!({ "_id": tpl, "_props": { "ShortName": short, "Name": name } }),
            );
            idx.insert(short.to_lowercase(), tpl.to_string());
        }
        Ctx {
            items: map,
            short_index: idx,
            presets: presets.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(),
        }
    }

    #[test]
    fn preset_id_wins_over_candidates() {
        let c = ctx(&[("p1", "AK-74M Default")], &[]);
        let def = Some(WeaponDef {
            preset: Some("p1".into()),
            label: Some("AK-74M".into()),
            candidates: vec!["不存在的名字".into()],
        });
        assert_eq!(resolve_weapon(&c, &def), Some(("p1".into(), "AK-74M".into())));
    }

    #[test]
    fn legacy_candidates_still_resolve_with_separator_difference() {
        let c = ctx(&[("p1", "AK-74M Default")], &[]);
        let def = Some(WeaponDef {
            preset: None,
            label: None,
            candidates: vec!["AK-74M_default".into()],
        });
        assert_eq!(resolve_weapon(&c, &def), Some(("p1".into(), "AK-74M Default".into())));
    }

    #[test]
    fn unknown_preset_falls_back_to_candidates() {
        let c = ctx(&[("p1", "AK-74M Default")], &[]);
        let def = Some(WeaponDef {
            preset: Some("stale".into()),
            label: None,
            candidates: vec!["AK-74M Default".into()],
        });
        assert_eq!(resolve_weapon(&c, &def), Some(("p1".into(), "AK-74M Default".into())));
    }

    #[test]
    fn piece_tpl_resolves_and_prefers_label() {
        let c = ctx(&[], &[("t1", ("gjel", "6B13"))]);
        let p = PieceDef {
            kind: "armor".into(),
            tpl: Some("t1".into()),
            label: Some("6B13 丛林".into()),
            candidates: vec![],
        };
        assert_eq!(resolve_piece(&c, &p), Some(("t1".into(), "6B13 丛林".into())));
    }

    #[test]
    fn unknown_piece_tpl_is_not_invented() {
        let c = ctx(&[], &[("t1", ("gjel", "6B13"))]);
        let p = PieceDef {
            kind: "armor".into(),
            tpl: Some("missing".into()),
            label: Some("配错的".into()),
            candidates: vec![],
        };
        assert_eq!(resolve_piece(&c, &p), None);
        assert_eq!(fallback_label(p.label.as_ref(), &p.candidates, p.tpl.as_ref()), "配错的");
    }

    #[test]
    fn bundled_config_only_uses_ids_or_candidates() {
        // 配置自洽性：每套必须有武器，且每个防具都有 tpl 或候选名
        let defs: Value = serde_json::from_str(SETS_JSON).unwrap();
        let sets = defs.get("sets").and_then(|v| v.as_array()).unwrap();
        assert_eq!(sets.len(), 12);
        for s in sets {
            let d: SetDef = serde_json::from_value(s.clone()).unwrap();
            let w = d.weapon.as_ref().expect("套装缺 weapon");
            assert!(w.preset.is_some() || !w.candidates.is_empty(), "{} 武器无标识", d.id);
            if let Some(p) = &w.preset {
                assert_eq!(p.len(), 24, "{} 武器 preset id 长度不对", d.id);
            }
            for p in &d.armor {
                assert!(p.tpl.is_some() || !p.candidates.is_empty(), "{} 的 {} 无标识", d.id, p.kind);
                if let Some(t) = &p.tpl {
                    assert_eq!(t.len(), 24, "{} 的 {} tpl 长度不对", d.id, p.kind);
                }
            }
        }
    }

    #[test]
    #[ignore = "需要本机 SPT 安装 E:\\EFT"]
    fn real_database_resolves_all_twelve_sets() {
        let rows = resolve_all(r"E:\EFT\SPT_Runtime\SPT_Data\database");
        assert_eq!(rows.len(), 12);
        let failed: Vec<_> = rows.iter().filter(|(_, ok)| !ok).map(|(id, _)| id.clone()).collect();
        assert!(failed.is_empty(), "以下套装有未解析项：{:?}", failed);
    }
}
