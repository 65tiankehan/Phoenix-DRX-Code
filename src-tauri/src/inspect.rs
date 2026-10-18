//! 检视 / 图鉴页（10-inspect）数据服务。
//! 结构对照参考编辑器（WeaponBuild.cs / ServerGlobals.cs）：
//! - 武器改装视图：以某物品为根收集档案子树（递归），指标按参考 CalculateBuildProperties
//!   口径聚合——根提供 Ergonomics / RecoilForceUp / RecoilForceBack 基础值，每个零件
//!   （在物品库时）累加 props.Ergonomics 与 props.Recoil（百分比增量），最终
//!   后坐力 = round(基础 + 基础 × ΣRecoil / 100)；has_modded = 任一子项不在物品库；
//!   零件列表 = 除根外的子项（跳过 cartridges / patron_in_weapon 弹药槽，稿图注 4）；
//! - 图鉴 = 服务器 globals.json config.ItemPresets（全局武器预设库，只读），名称 =
//!   根武器本地化名 + locale[预设 Id]（缺失仅武器名，参考 CalculateBuildProperties）；
//! - 加入预设 = 存入档案 userbuilds.weaponBuilds（参考 ImportBuild）；
//! - 导出 = 预设 / 档案武器子树 JSON 写入用户选定路径（系统文件选择白名单例外）。

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::Serialize;
use serde_json::{json, Value};
use tauri::AppHandle;

use crate::character::database_dir;
use crate::profile::{is_safe_id, profiles_dir};

/// 零件行（弹匣内弹药已跳过；slotId 原值展示）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildPartRow {
    pub id: String,
    pub name: String,
    pub tpl: String,
    pub slot_id: String,
    pub modded: bool,
}

/// 武器改装视图（参考 WeaponBuildView 四行表：指标三行 + 零件列表）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildView {
    pub id: String,
    pub name: String,
    pub root_tpl: String,
    pub root_name: String,
    pub ergonomics: i64,
    pub recoil_up: i64,
    pub recoil_back: i64,
    /// 预设 / 子树含不在物品库的模板（红三角）
    pub has_modded: bool,
    pub parts: Vec<BuildPartRow>,
}

/// 跳过的弹药槽位（稿图注 4：弹匣内弹药不计入零件）
const SKIP_SLOTS: &[&str] = &["cartridges", "patron_in_weapon"];

// ———————— 读侧 helpers ————————

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

/// 物品库（tpl → _props 关键字段）
pub(crate) struct InspectDb {
    props: HashMap<String, ItemProps>,
}

#[derive(Debug, Clone, Copy)]
struct ItemProps {
    ergonomics: f64,
    recoil: f64,
    recoil_up: f64,
    recoil_back: f64,
}

impl InspectDb {
    pub(crate) fn load(app: &AppHandle) -> InspectDb {
        let mut props = HashMap::new();
        if let Some(db) = database_dir(app) {
            if let Ok(txt) =
                fs::read_to_string(Path::new(&db).join("templates").join("items.json"))
            {
                if let Ok(Value::Array(arr)) = serde_json::from_str::<Value>(&txt) {
                    for it in &arr {
                        if let Some(tpl) = it.get("_id").and_then(|v| v.as_str()) {
                            let p = it.get("_props");
                            props.insert(
                                tpl.to_string(),
                                ItemProps {
                                    ergonomics: p.and_then(|x| x.get("Ergonomics")).and_then(|v| v.as_f64()).unwrap_or(0.0),
                                    recoil: p.and_then(|x| x.get("Recoil")).and_then(|v| v.as_f64()).unwrap_or(0.0),
                                    recoil_up: p.and_then(|x| x.get("RecoilForceUp")).and_then(|v| v.as_f64()).unwrap_or(0.0),
                                    recoil_back: p
                                        .and_then(|x| x.get("RecoilForceBack"))
                                        .and_then(|v| v.as_f64())
                                        .unwrap_or(0.0),
                                },
                            );
                        }
                    }
                }
            }
        }
        InspectDb { props }
    }

    fn get(&self, tpl: &str) -> Option<ItemProps> {
        self.props.get(tpl).copied()
    }
}

fn localized(names: Option<&Value>, key: &str, fallback: &str) -> String {
    names
        .and_then(|n| n.get(key))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| fallback.to_string())
}

/// 武器改装视图聚合（参考 CalculateBuildProperties；items 含根）
pub(crate) fn build_view(
    id: &str,
    name: &str,
    root_id: &str,
    items: &[Value],
    db: &InspectDb,
    names: Option<&Value>,
) -> Option<BuildView> {
    let root = items.iter().find(|it| it.get("_id").and_then(|v| v.as_str()) == Some(root_id))?;
    let root_tpl = root.get("_tpl").and_then(|v| v.as_str())?;
    let root_name = localized(names, format!("{} Name", root_tpl).as_str(), root_tpl);

    let base = db.get(root_tpl);
    let mut ergonomics = base.map(|p| p.ergonomics).unwrap_or(0.0);
    let mut recoil_up = base.map(|p| p.recoil_up).unwrap_or(0.0);
    let mut recoil_back = base.map(|p| p.recoil_back).unwrap_or(0.0);
    let mut recoil_delta = 0.0f64;
    let mut has_modded = false;

    let mut parts: Vec<BuildPartRow> = Vec::new();
    for it in items {
        let Some(it_id) = it.get("_id").and_then(|v| v.as_str()) else {
            continue;
        };
        if it_id == root_id {
            continue;
        }
        let tpl = it.get("_tpl").and_then(|v| v.as_str()).unwrap_or("");
        let slot_id = it.get("slotId").and_then(|v| v.as_str()).unwrap_or("");
        let in_db = db.get(tpl).is_some();
        has_modded = has_modded || !in_db;
        if !SKIP_SLOTS.contains(&slot_id) {
            parts.push(BuildPartRow {
                id: it_id.to_string(),
                name: localized(names, format!("{} Name", tpl).as_str(), tpl),
                tpl: tpl.to_string(),
                slot_id: slot_id.to_string(),
                modded: !in_db,
            });
        }
        if in_db {
            let p = db.get(tpl).unwrap();
            recoil_delta += p.recoil;
            ergonomics += p.ergonomics;
        }
    }
    recoil_delta /= 100.0;
    recoil_up = (recoil_up + recoil_up * recoil_delta).round();
    recoil_back = (recoil_back + recoil_back * recoil_delta).round();

    Some(BuildView {
        id: id.to_string(),
        name: name.to_string(),
        root_tpl: root_tpl.to_string(),
        root_name,
        ergonomics: ergonomics.round() as i64,
        recoil_up: recoil_up as i64,
        recoil_back: recoil_back as i64,
        has_modded,
        parts,
    })
}

/// 收集物品子树（根 + 全部递归子孙，参考 WeaponBuild(InventoryItem, items) 口径）
fn collect_subtree(items: &[Value], root_id: &str) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    let mut todo: Vec<String> = vec![root_id.to_string()];
    while let Some(pid) = todo.pop() {
        for it in items {
            let Some(id) = it.get("_id").and_then(|v| v.as_str()) else {
                continue;
            };
            let parent = it.get("parentId").and_then(|v| v.as_str()).unwrap_or("");
            if id == pid || parent == pid {
                if !out.iter().any(|x| x.get("_id").and_then(|v| v.as_str()) == Some(id)) {
                    todo.push(id.to_string());
                    out.push(it.clone());
                }
            }
        }
    }
    out
}

fn load_profile(app: &AppHandle, id: &str) -> Option<Value> {
    let dir = profiles_dir(app)?;
    let raw: Value = serde_json::from_str(
        &fs::read_to_string(Path::new(&dir).join(format!("{}.json", id))).ok()?,
    )
    .ok()?;
    Some(raw)
}

// ———————— 命令实现 ————————

/// 检视档案内武器：收集子树并聚合指标；查无或无物品库根时为 None
pub fn inspect_weapon(app: &AppHandle, profile_id: &str, item_id: &str) -> Option<BuildView> {
    if !is_safe_id(profile_id) {
        return None;
    }
    let raw = load_profile(app, profile_id)?;
    let items = raw
        .pointer("/characters/pmc/Inventory/items")?
        .as_array()?
        .clone();
    let lang = raw
        .pointer("/info/language")
        .and_then(|v| v.as_str())
        .unwrap_or("en")
        .to_string();
    let names = locales_table(app, &lang);
    let db = InspectDb::load(app);
    let subtree = collect_subtree(&items, item_id);
    if subtree.is_empty() {
        return None;
    }
    let root = subtree
        .iter()
        .find(|it| it.get("_id").and_then(|v| v.as_str()) == Some(item_id))?;
    let root_tpl = root.get("_tpl").and_then(|v| v.as_str())?;
    let name = localized(names.as_ref(), format!("{} Name", root_tpl).as_str(), root_tpl);
    build_view(item_id, &name, item_id, &subtree, &db, names.as_ref())
}

/// 图鉴：服务器 globals.json config.ItemPresets → 全局武器预设视图（只读）
pub fn builds_global(app: &AppHandle) -> Vec<BuildView> {
    let Some(db_dir) = database_dir(app) else {
        return Vec::new();
    };
    let Ok(txt) = fs::read_to_string(Path::new(&db_dir).join("globals.json")) else {
        return Vec::new();
    };
    let Ok(v) = serde_json::from_str::<Value>(&txt) else {
        return Vec::new();
    };
    let Some(presets) = v.pointer("/config/ItemPresets").and_then(|p| p.as_object()) else {
        return Vec::new();
    };
    let db = InspectDb::load(app);
    // 语言取不到档案（本命令无档案上下文），与检视统一走 en 回退链
    let names = locales_table(app, "en");
    let mut out: Vec<BuildView> = Vec::new();
    for (pid, preset) in presets {
        let Some(root_id) = preset.get("root").and_then(|r| r.as_str()) else {
            continue;
        };
        let Some(items) = preset.get("items").and_then(|i| i.as_array()) else {
            continue;
        };
        // 名称 = 根武器本地化名 + locale[预设 Id]（缺失仅武器名，参考口径）
        let root_tpl = items
            .iter()
            .find(|it| it.get("_id").and_then(|x| x.as_str()) == Some(root_id))
            .and_then(|it| it.get("_tpl").and_then(|x| x.as_str()))
            .unwrap_or("");
        let weapon_name = localized(names.as_ref(), format!("{} Name", root_tpl).as_str(), root_tpl);
        let name = match names.as_ref().and_then(|n| n.get(pid)).and_then(|x| x.as_str()) {
            Some(extra) => format!("{} {}", weapon_name, extra),
            None => weapon_name,
        };
        if let Some(view) = build_view(pid, &name, root_id, items, &db, names.as_ref()) {
            out.push(view);
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// 加入预设：把档案内武器子树存入 userbuilds.weaponBuilds（参考 ImportBuild）
pub fn preset_add(app: &AppHandle, profile_id: &str, item_id: &str) -> Result<(), String> {
    if !is_safe_id(profile_id) {
        return Err(format!("Invalid profile id: {}", profile_id));
    }
    let dir = profiles_dir(app).ok_or_else(|| "未选择 SPT 服务器".to_string())?;
    let path = Path::new(&dir).join(format!("{}.json", profile_id));
    let txt = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut raw: Value = serde_json::from_str(&txt).map_err(|e| e.to_string())?;
    // 写档案铁律：先备份
    crate::backup::backup_before_write(app, &path)?;

    let items = raw
        .pointer("/characters/pmc/Inventory/items")
        .and_then(|v| v.as_array())
        .ok_or("Inventory.items 缺失")?;
    let subtree = collect_subtree(items, item_id);
    if subtree.is_empty() {
        return Err("查无该武器".to_string());
    }

    let lang = raw
        .pointer("/info/language")
        .and_then(|v| v.as_str())
        .unwrap_or("en")
        .to_string();
    let names = locales_table(app, &lang);
    let root_tpl = subtree
        .iter()
        .find(|it| it.get("_id").and_then(|v| v.as_str()) == Some(item_id))
        .and_then(|it| it.get("_tpl").and_then(|v| v.as_str()))
        .unwrap_or("");
    let name = localized(names.as_ref(), format!("{} Name", root_tpl).as_str(), root_tpl);

    let build = json!({
        "id": item_id,
        "name": name,
        "root": item_id,
        "items": subtree
    });
    if raw.pointer("/userbuilds/weaponBuilds").is_none() {
        if let Some(ub) = raw.pointer_mut("/userbuilds") {
            ub["weaponBuilds"] = json!([]);
        }
    }
    let builds = raw
        .pointer_mut("/userbuilds/weaponBuilds")
        .and_then(|v| v.as_array_mut())
        .ok_or("userbuilds 缺失")?;
    if builds
        .iter()
        .any(|b| b.get("id").and_then(|v| v.as_str()) == Some(item_id))
    {
        return Err("该武器已在用户预设中".to_string());
    }
    builds.push(build);
    let out = serde_json::to_string_pretty(&raw).map_err(|e| e.to_string())?;
    fs::write(&path, out).map_err(|e| e.to_string())
}

/// 导出全局预设 JSON 到用户选定路径（系统文件选择属白名单例外）
pub fn preset_export(path: &str, preset_id: &str, app: &AppHandle) -> Result<(), String> {
    let Some(db_dir) = database_dir(app) else {
        return Err("未选择 SPT 服务器".to_string());
    };
    let txt = fs::read_to_string(Path::new(&db_dir).join("globals.json")).map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&txt).map_err(|e| e.to_string())?;
    let preset = v
        .pointer("/config/ItemPresets")
        .and_then(|p| p.get(preset_id))
        .ok_or("查无该预设")?;
    fs::write(path, serde_json::to_string_pretty(preset).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

/// 导出档案内武器子树 JSON 到用户选定路径
pub fn weapon_export(
    app: &AppHandle,
    profile_id: &str,
    item_id: &str,
    path: &str,
) -> Result<(), String> {
    if !is_safe_id(profile_id) {
        return Err(format!("Invalid profile id: {}", profile_id));
    }
    let raw = load_profile(app, profile_id).ok_or("档案缺失")?;
    let items = raw
        .pointer("/characters/pmc/Inventory/items")
        .and_then(|v| v.as_array())
        .ok_or("Inventory.items 缺失")?;
    let subtree = collect_subtree(items, item_id);
    if subtree.is_empty() {
        return Err("查无该武器".to_string());
    }
    fs::write(
        path,
        serde_json::to_string_pretty(&json!({ "root": item_id, "items": subtree }))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
