//! 预设页（12-presets）数据服务 —— 用户预设（武器 / 装备）读写。
//! 结构对照参考编辑器（UserBuilds.cs / Build.cs / BuildsTabViewModel.cs）：
//! - 读侧：userbuilds.weaponBuilds → 武器走 inspect::build_view 聚合指标（与图鉴同一
//!   视图口径）；userbuilds.equipmentBuilds → 13 槽（12 固定 + 口袋）逐槽定位
//!   （items 中 parentId == build.root 且 slotId 匹配），口袋容器 = Pockets 槽物品，
//!   口袋内物品挂容器 id 下（同 11-scav 装备区口径）；build id 读侧兼容 "_id" 回退；
//! - 导入（多选 JSON，参考 GetImportPaths）：逐文件校验——武器 name 非空 + 根模板
//!   是武器（RecoilForceUp ≠ 0，参考 Build.IsWeapon）；装备 name 非空 + BuildType ==
//!   "Custom"；失败文件名进 failed（参考 NoBuildInFileException）；重名 "{0}({1})"
//!   递增（count 从 1，参考 PrepareForImport）+ id 全局避重；items 原样保留；
//!   全部处理完 imported > 0 才备份 + 写档案（一次备份）；
//! - 导出：单个写原样 build（保留 fastPanel / BuildType 等，re-import 无损；偏离参考
//!   的 RemoveNullAndEmptyProperties 清理，注释留档）；全部逐个写 "{前缀}-{名称}.json"
//!   （文件名非法字符替换下划线；重名覆盖，同参考 ExportAll）；
//! - 删除：单个 retain / 全部 clear（参考 RemoveWeaponBuild(s) / RemoveEquipmentBuild(s)）；
//! - 加入仓库：复用 stash::place_build_tree 放置引擎（根走 find_free_slot，零件挂
//!   根下原 slotId，id 重映射）；
//! - 写档案铁律：任何写入前先 backup_before_write（领域铁律：备份先行）。

use std::fs;
use std::iter::once;
use std::path::Path;

use serde::Serialize;
use serde_json::{json, Value};
use tauri::AppHandle;

use crate::backup::backup_before_write;
use crate::character::database_dir;
use crate::inspect::{build_view, InspectDb};
use crate::profile::{is_safe_id, profiles_dir, SaveResult};
use crate::stash::{
    item_row, locales_table, new_item_id, place_build_tree, EquipSlotRow, ItemsDb, StashItemRow,
    EQUIP_SLOTS,
};

// ———————— 类型 ————————

/// 装备预设视图：13 槽（12 固定 + 口袋）+ 口袋内物品 + 红三角标记
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EquipmentPresetView {
    pub id: String,
    pub name: String,
    /// 槽序即稿图 13 槽序（item 为 None 即空槽）
    pub slots: Vec<EquipSlotRow>,
    /// 口袋容器（Pockets 槽物品）内的物品
    pub pockets: Vec<StashItemRow>,
    /// 任一槽 / 口袋物品不在物品库（红三角）
    pub has_modded: bool,
}

/// 预设页全量数据（武器 + 装备两个子页）
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetsData {
    pub weapons: Vec<crate::inspect::BuildView>,
    pub equipment: Vec<EquipmentPresetView>,
}

/// 导入结果（failed = 校验失败文件名列表，前端对照 tab_presets_file_not_build 提示）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub imported: usize,
    pub failed: Vec<String>,
    /// imported > 0 时才有备份文件
    pub backup_path: Option<String>,
}

// ———————— 读侧 helpers ————————

/// build id：本项目写侧用 "id" 键（preset_add 先例），读侧兼容 "_id" 回退
fn build_id(b: &Value) -> Option<&str> {
    b.get("id")
        .and_then(|v| v.as_str())
        .or_else(|| b.get("_id").and_then(|v| v.as_str()))
}

fn build_name(b: &Value) -> Option<&str> {
    b.get("name").and_then(|v| v.as_str())
}

fn find_build<'a>(builds: &'a [Value], id: &str) -> Option<&'a Value> {
    builds.iter().find(|b| build_id(b) == Some(id))
}

/// 子页 kind → (userbuilds 指针, 导出文件名前缀)
fn kind_of(kind: &str) -> Option<(&'static str, &'static str)> {
    match kind {
        "weapon" => Some(("/userbuilds/weaponBuilds", "武器预设")),
        "equipment" => Some(("/userbuilds/equipmentBuilds", "装备预设")),
        _ => None,
    }
}

fn load_profile(app: &AppHandle, id: &str) -> Option<Value> {
    let dir = profiles_dir(app)?;
    let raw: Value = serde_json::from_str(
        &fs::read_to_string(Path::new(&dir).join(format!("{}.json", id))).ok()?,
    )
    .ok()?;
    Some(raw)
}

/// 装备预设视图聚合：EQUIP_SLOTS 12 槽 + 口袋槽逐槽定位，口袋内物品挂容器 id 下
fn equipment_view(b: &Value, db: &ItemsDb) -> Option<EquipmentPresetView> {
    let id = build_id(b)?.to_string();
    let name = build_name(b)?.to_string();
    let root = b.get("root").and_then(|v| v.as_str()).unwrap_or("");
    let items = b.get("items").and_then(|v| v.as_array())?;

    let row_of = |it: &Value| -> Option<StashItemRow> {
        Some(item_row(
            it.get("_id")?.as_str()?,
            it.get("_tpl")?.as_str()?,
            Some(it),
            db,
        ))
    };
    let in_slot = |slot: &str| -> Option<StashItemRow> {
        items
            .iter()
            .find(|it| {
                it.get("parentId").and_then(|v| v.as_str()) == Some(root)
                    && it.get("slotId").and_then(|v| v.as_str()) == Some(slot)
            })
            .and_then(row_of)
    };

    let mut has_modded = false;
    let slots: Vec<EquipSlotRow> = EQUIP_SLOTS
        .iter()
        .chain(once(&"Pockets"))
        .map(|slot| {
            let item = in_slot(slot);
            if let Some(it) = &item {
                has_modded = has_modded || it.modded;
            }
            EquipSlotRow {
                slot: slot.to_string(),
                item,
            }
        })
        .collect();

    // 口袋容器（Pockets 槽物品）内的直接子项
    let pockets: Vec<StashItemRow> = match in_slot("Pockets") {
        Some(container) => items
            .iter()
            .filter(|it| {
                it.get("parentId").and_then(|v| v.as_str()) == Some(container.id.as_str())
            })
            .filter_map(row_of)
            .collect(),
        None => Vec::new(),
    };
    has_modded = has_modded || pockets.iter().any(|p| p.modded);

    Some(EquipmentPresetView {
        id,
        name,
        slots,
        pockets,
        has_modded,
    })
}

/// 预设页全量数据；档案缺失或不可读时为 None
pub fn load_presets(app: &AppHandle, id: &str) -> Option<PresetsData> {
    if !is_safe_id(id) {
        return None;
    }
    let raw = load_profile(app, id)?;
    let lang = raw
        .pointer("/info/language")
        .and_then(|v| v.as_str())
        .unwrap_or("en")
        .to_string();
    let names = locales_table(app, &lang);
    let idb = InspectDb::load(app);
    let sdb = ItemsDb::load(app, names.as_ref());

    // 武器预设：build_view 聚合指标（与图鉴 / 检视同一视图口径）
    let weapons: Vec<crate::inspect::BuildView> = raw
        .pointer("/userbuilds/weaponBuilds")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|b| {
                    let bid = build_id(b)?;
                    let name = build_name(b).unwrap_or("");
                    let root = b.get("root").and_then(|v| v.as_str()).unwrap_or(bid);
                    let items = b.get("items").and_then(|v| v.as_array())?;
                    build_view(bid, name, root, items, &idb, names.as_ref())
                })
                .collect()
        })
        .unwrap_or_default();

    // 装备预设：逐个 13 槽视图（读侧不按 BuildType 过滤，导入时才校验）
    let equipment: Vec<EquipmentPresetView> = raw
        .pointer("/userbuilds/equipmentBuilds")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|b| equipment_view(b, &sdb)).collect())
        .unwrap_or_default();

    Some(PresetsData { weapons, equipment })
}

// ———————— 写侧 helpers ————————

/// userbuilds 子数组取用（缺失时按需补齐，写侧先例 preset_add）
fn builds_arr_mut<'a>(raw: &'a mut Value, pointer: &str) -> Result<&'a mut Vec<Value>, String> {
    if raw.pointer(pointer).is_none() {
        let (parent, leaf) = pointer.rsplit_once('/').unwrap_or(("", pointer));
        if let Some(p) = raw.pointer_mut(parent) {
            p[leaf] = json!([]);
        }
    }
    raw.pointer_mut(pointer)
        .and_then(|v| v.as_array_mut())
        .ok_or("userbuilds 缺失".to_string())
}

/// 档案写回统一封装：读 → 备份 → 变更 → pretty 写回（领域铁律：备份先行）
fn with_profile(
    app: &AppHandle,
    id: &str,
    f: impl FnOnce(&mut Value) -> Result<(), String>,
) -> Result<SaveResult, String> {
    if !is_safe_id(id) {
        return Err(format!("Invalid profile id: {}", id));
    }
    let dir = profiles_dir(app).ok_or_else(|| "未选择 SPT 服务器".to_string())?;
    let path = Path::new(&dir).join(format!("{}.json", id));
    let txt = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut raw: Value = serde_json::from_str(&txt).map_err(|e| e.to_string())?;
    let backup_path = backup_before_write(app, &path)?;
    f(&mut raw)?;
    let out = serde_json::to_string_pretty(&raw).map_err(|e| e.to_string())?;
    fs::write(&path, out).map_err(|e| e.to_string())?;
    Ok(SaveResult {
        backup_path: backup_path.map(|p| p.to_string_lossy().to_string()),
    })
}

/// 重名处理："{0}({1})" 递增（count 从 1，参考 PrepareForImport）
fn dedupe_name(name: &str, used: &[String]) -> String {
    let mut candidate = name.to_string();
    let mut count = 1;
    while used.iter().any(|n| n == &candidate) {
        candidate = format!("{}({})", name, count);
        count += 1;
    }
    candidate
}

/// 新 id 全局避重（参考 ExtMethods.GenerateNewId(existBuilds.Select(x => x.Id))）
fn unique_id(used: &mut Vec<String>) -> String {
    loop {
        let cand = new_item_id(used.len() as u32);
        if !used.contains(&cand) {
            used.push(cand.clone());
            return cand;
        }
    }
}

/// 文件名清洗：Windows 非法字符替换下划线（参考未处理，此处防路径炸裂）
fn safe_filename(name: &str) -> String {
    name.chars()
        .map(|c| if "\\/:*?\"<>|".contains(c) { '_' } else { c })
        .collect()
}

// ———————— 写侧命令 ————————

/// 删除单个预设（参考 RemoveWeaponBuild / RemoveEquipmentBuild）
pub fn preset_remove(
    app: &AppHandle,
    id: &str,
    kind: &str,
    target: &str,
) -> Result<SaveResult, String> {
    let (pointer, _) = kind_of(kind).ok_or("未知预设类型")?;
    with_profile(app, id, |raw| {
        let arr = builds_arr_mut(raw, pointer)?;
        let n0 = arr.len();
        arr.retain(|b| build_id(b) != Some(target));
        if arr.len() == n0 {
            return Err("查无该预设".to_string());
        }
        Ok(())
    })
}

/// 清空当前子页全部预设（参考 RemoveWeaponBuilds / RemoveEquipmentBuilds）
pub fn builds_remove_all(app: &AppHandle, id: &str, kind: &str) -> Result<SaveResult, String> {
    let (pointer, _) = kind_of(kind).ok_or("未知预设类型")?;
    with_profile(app, id, |raw| {
        builds_arr_mut(raw, pointer)?.clear();
        Ok(())
    })
}

/// 把用户预设整树复制进仓库：根走放置引擎，零件挂根下原 slotId，id 重映射；
/// 先 clone (root, items) 再进可变闭包，避免读写借用冲突
pub fn build_to_stash(
    app: &AppHandle,
    id: &str,
    kind: &str,
    target: &str,
) -> Result<SaveResult, String> {
    let (pointer, _) = kind_of(kind).ok_or("未知预设类型")?;
    let Some(db_dir) = database_dir(app) else {
        return Err("未选择 SPT 服务器".to_string());
    };
    let raw0 = load_profile(app, id).ok_or("档案缺失")?;
    let build = raw0
        .pointer(pointer)
        .and_then(|v| v.as_array())
        .and_then(|arr| find_build(arr, target))
        .ok_or("查无该预设")?
        .clone();
    let root_id = build
        .get("root")
        .and_then(|v| v.as_str())
        .unwrap_or(target)
        .to_string();
    let items: Vec<Value> = build
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .ok_or("预设缺物品表")?;
    let stash_id = raw0
        .pointer("/characters/pmc/Inventory/stash")
        .and_then(|v| v.as_str())
        .ok_or("Inventory.stash 缺失")?
        .to_string();
    let lang = raw0
        .pointer("/info/language")
        .and_then(|v| v.as_str())
        .unwrap_or("en")
        .to_string();
    let names = locales_table(app, &lang);
    let db = ItemsDb::load(app, names.as_ref());

    with_profile(app, id, move |raw| {
        place_build_tree(raw, &stash_id, &root_id, &items, &db_dir, &db, 0)
    })
}

/// 导入预设 JSON（多选文件；参考 GetImportPaths + Import*BuildFromFile +
/// PrepareForImport）。先全部校验 / 改名 / 重编 id，imported > 0 才一次备份写入；
/// items 原样保留（偏离参考的 items 清洗，保 re-import 无损）
pub fn build_import(
    app: &AppHandle,
    id: &str,
    kind: &str,
    paths: Vec<String>,
) -> Result<ImportResult, String> {
    let (pointer, _) = kind_of(kind).ok_or("未知预设类型")?;
    if !is_safe_id(id) {
        return Err(format!("Invalid profile id: {}", id));
    }
    let raw0 = load_profile(app, id).ok_or("档案缺失")?;
    let lang = raw0
        .pointer("/info/language")
        .and_then(|v| v.as_str())
        .unwrap_or("en")
        .to_string();
    let names = locales_table(app, &lang);
    let db = ItemsDb::load(app, names.as_ref());

    let existing = raw0
        .pointer(pointer)
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut used_ids: Vec<String> = existing
        .iter()
        .filter_map(|b| build_id(b).map(str::to_string))
        .collect();
    let mut used_names: Vec<String> = existing
        .iter()
        .filter_map(|b| build_name(b).map(str::to_string))
        .collect();

    let mut imported: Vec<Value> = Vec::new();
    let mut failed: Vec<String> = Vec::new();
    for p in &paths {
        let fname = Path::new(p)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| p.clone());
        let parsed = fs::read_to_string(p)
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok());
        let Some(mut b) = parsed else {
            failed.push(fname);
            continue;
        };
        // 校验（参考 ImportWeaponBuildFromFile / ImportEquipmentBuildFromFile）
        let ok = match kind {
            "weapon" => {
                let root = b.get("root").and_then(|v| v.as_str()).unwrap_or("");
                b.get("items")
                    .and_then(|v| v.as_array())
                    .and_then(|items| {
                        items.iter().find(|it| {
                            it.get("_id").and_then(|v| v.as_str()) == Some(root)
                        })
                    })
                    .and_then(|it| it.get("_tpl").and_then(|v| v.as_str()))
                    .map(|tpl| db.is_weapon(tpl))
                    .unwrap_or(false)
            }
            "equipment" => b.get("BuildType").and_then(|v| v.as_str()) == Some("Custom"),
            _ => false,
        } && build_name(&b).is_some();
        if !ok {
            failed.push(fname);
            continue;
        }
        // PrepareForImport：重名递增 + 新 id；items 原样保留
        let name = dedupe_name(build_name(&b).unwrap_or(""), &used_names);
        used_names.push(name.clone());
        let bid = unique_id(&mut used_ids);
        if let Some(obj) = b.as_object_mut() {
            obj.remove("_id");
            obj.insert("id".into(), json!(bid));
            obj.insert("name".into(), json!(name));
        }
        imported.push(b);
    }

    if imported.is_empty() {
        return Ok(ImportResult {
            imported: 0,
            failed,
            backup_path: None,
        });
    }
    let dir = profiles_dir(app).ok_or_else(|| "未选择 SPT 服务器".to_string())?;
    let path = Path::new(&dir).join(format!("{}.json", id));
    let txt = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut raw: Value = serde_json::from_str(&txt).map_err(|e| e.to_string())?;
    let backup_path = backup_before_write(app, &path)?;
    let imported_n = imported.len();
    builds_arr_mut(&mut raw, pointer)?.extend(imported);
    let out = serde_json::to_string_pretty(&raw).map_err(|e| e.to_string())?;
    fs::write(&path, out).map_err(|e| e.to_string())?;
    Ok(ImportResult {
        imported: imported_n,
        failed,
        backup_path: backup_path.map(|p| p.to_string_lossy().to_string()),
    })
}

/// 导出单个预设 JSON 到用户选定路径（原样 build 对象，保留 fastPanel / BuildType）
pub fn build_export(
    app: &AppHandle,
    id: &str,
    kind: &str,
    target: &str,
    path: &str,
) -> Result<(), String> {
    let (pointer, _) = kind_of(kind).ok_or("未知预设类型")?;
    let raw = load_profile(app, id).ok_or("档案缺失")?;
    let build = raw
        .pointer(pointer)
        .and_then(|v| v.as_array())
        .and_then(|arr| find_build(arr, target))
        .ok_or("查无该预设")?;
    fs::write(
        path,
        serde_json::to_string_pretty(build).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

/// 导出当前子页全部预设到目录：逐个写 "{前缀}-{名称}.json"（重名覆盖，
/// 同参考 ExportAll 逐个写）；返回成功条数
pub fn builds_export_all(
    app: &AppHandle,
    id: &str,
    kind: &str,
    dir: &str,
) -> Result<usize, String> {
    let (pointer, prefix) = kind_of(kind).ok_or("未知预设类型")?;
    let raw = load_profile(app, id).ok_or("档案缺失")?;
    let builds = raw
        .pointer(pointer)
        .and_then(|v| v.as_array())
        .ok_or("userbuilds 缺失")?;
    let mut n = 0;
    for b in builds {
        let Some(bid) = build_id(b) else {
            continue;
        };
        let name = build_name(b).unwrap_or(bid);
        let path = Path::new(dir).join(format!("{}-{}.json", prefix, safe_filename(name)));
        fs::write(
            &path,
            serde_json::to_string_pretty(b).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        n += 1;
    }
    Ok(n)
}
