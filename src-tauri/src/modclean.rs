//! 清理 mod 物品（参考 SkiTles55 4.1.1 `Views\CleaningFromMods.xaml` 与
//! `Core\CleaningService.cs`）。
//!
//! 「mod 物品」的判据只有一条：**存档里引用的 id 在服务器数据库里找不到**。
//! 参考编辑器扫七类（PMC/Scav 背包物品、任务、已检视、商人、武器预设、装备预设）；
//! 本项目按真实档结构覆盖其中可安全自动处理的四类：
//!
//!   1. 背包物品（PMC / Scav 的 `Inventory.items[]`）—— `_tpl` 不在 `templates/items.json`
//!   2. 已检视（`characters.pmc.Encyclopedia` 键）—— 键不在 items.json
//!   3. 任务（`characters.pmc.Quests[].qid`）—— qid 不在 `templates/quests.json`
//!   4. 预设（`userbuilds.weaponBuilds / equipmentBuilds / magazineBuilds`）
//!      —— 预设 `items[]._tpl` 有任一不在 items.json
//!
//! 商人分组（参考的第五类）在本项目里没有对应 UI（真实档 `TraderStandings` 为空），
//! 不纳入。物品被移除时连带移除它的子件（弹匣里的子弹、容器里的东西），否则会留下孤儿条目。
//!
//! 写入前一律走 `backup_before_write`（章程 §四 铁律）。

use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::fs;
use tauri::AppHandle;

use crate::backup::backup_before_write;
use crate::character::database_dir;
use crate::profile::get_current_profile_id;
use crate::profile::profiles_dir;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModCleanData {
    /// 背包物品（含子件）里 _tpl 缺失的条目数
    pub items: usize,
    /// Encyclopedia 里 tpl 缺失的键数
    pub encyclopedia: usize,
    /// Quests 里 qid 缺失的条目数
    pub quests: usize,
    /// 含缺失物品的预设数
    pub builds: usize,
    pub total: usize,
    /// 前若干条给用户确认用（`tpl / qid`）
    pub samples: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModCleanResult {
    pub removed: usize,
    /// 备份文件路径（可空：无改动时不写盘）
    pub backup: Option<String>,
}

struct Target {
    path: std::path::PathBuf,
    root: Value,
}

fn profile_target(app: &AppHandle) -> Result<Target, String> {
    let id = get_current_profile_id(app).ok_or("未选择存档")?;
    let dir = profiles_dir(app).ok_or("未选择 SPT 服务器")?;
    let path = std::path::Path::new(&dir).join(format!("{}.json", id));
    let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let root: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(Target { path, root })
}

/// 数据库已知 tpl 全集（items.json 全部键，含 Node 分类节点 —— 节点不会被当物品引用）
fn known_tpls(app: &AppHandle) -> Result<HashSet<String>, String> {
    let db = database_dir(app).ok_or("未找到服务器数据库目录")?;
    let path = std::path::Path::new(&db).join("templates").join("items.json");
    let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let map: Map<String, Value> = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(map.keys().cloned().collect())
}

/// 数据库已知任务 id 全集
fn known_quests(app: &AppHandle) -> Result<HashSet<String>, String> {
    let db = database_dir(app).ok_or("未找到服务器数据库目录")?;
    let path = std::path::Path::new(&db).join("templates").join("quests.json");
    let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let map: Map<String, Value> = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(map.keys().cloned().collect())
}

/// 背包物品里 tpl 缺失的 id（不含子件）
fn modded_item_ids(inv_items: &[Value], known: &HashSet<String>) -> Vec<String> {
    inv_items
        .iter()
        .filter(|it| {
            it.get("_tpl")
                .and_then(|v| v.as_str())
                .map(|tpl| !known.contains(tpl))
                .unwrap_or(false)
        })
        .filter_map(|it| it.get("_id").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .collect()
}

fn modded_quest_ids(quests: &[Value], known: &HashSet<String>) -> Vec<String> {
    quests
        .iter()
        .filter(|q| {
            q.get("qid")
                .and_then(|v| v.as_str())
                .map(|qid| !known.contains(qid))
                .unwrap_or(false)
        })
        .filter_map(|q| q.get("qid").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .collect()
}

/// 预设里出现缺失 tpl 的预设 id（三类预设统一）
fn modded_build_ids(userbuilds: &Value, known: &HashSet<String>) -> Vec<(String, String)> {
    const KINDS: [&str; 3] = ["weaponBuilds", "equipmentBuilds", "magazineBuilds"];
    let mut out = Vec::new();
    for kind in KINDS {
        let Some(list) = userbuilds.get(kind).and_then(|v| v.as_array()) else {
            continue;
        };
        for build in list {
            let hit = build
                .get("items")
                .and_then(|v| v.as_array())
                .map(|items| {
                    items.iter().any(|it| {
                        it.get("_tpl")
                            .and_then(|v| v.as_str())
                            .map(|tpl| !known.contains(tpl))
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
            if hit {
                if let Some(id) = build.get("_id").and_then(|v| v.as_str()) {
                    out.push((kind.to_string(), id.to_string()));
                }
            }
        }
    }
    out
}

fn inv_items_of<'a>(root: &'a Value, who: &str) -> &'a [Value] {
    root.pointer(&format!("/characters/{}/Inventory/items", who))
        .and_then(|v| v.as_array())
        .map(|v| v.as_slice())
        .unwrap_or(&[])
}

fn quests_of<'a>(root: &'a Value, who: &str) -> &'a [Value] {
    root.pointer(&format!("/characters/{}/Quests", who))
        .and_then(|v| v.as_array())
        .map(|v| v.as_slice())
        .unwrap_or(&[])
}

/// 扫描：返回四类计数 + 前 8 条样本（`tpl` / `qid` / 预设 id）
#[tauri::command]
pub async fn mods_scan(app: AppHandle) -> Result<ModCleanData, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let target = profile_target(&app)?;
        let known = known_tpls(&app)?;
        let known_q = known_quests(&app)?;

        let mut samples: Vec<String> = Vec::new();

        let pmc_items = inv_items_of(&target.root, "pmc");
        let scav_items = inv_items_of(&target.root, "scav");
        let mut item_ids = modded_item_ids(pmc_items, &known);
        item_ids.extend(modded_item_ids(scav_items, &known));
        // 连带子件，计数按去重后的实际删除量算
        let removed_items = crate::stash::collect_subtree(&item_ids, pmc_items)
            .into_iter()
            .chain(crate::stash::collect_subtree(&item_ids, scav_items))
            .collect::<HashSet<String>>();

        let enc_modded: Vec<String> = target
            .root
            .pointer("/characters/pmc/Encyclopedia")
            .and_then(|v| v.as_object())
            .map(|m| {
                m.keys()
                    .filter(|k| !known.contains(*k))
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let pmc_quests = modded_quest_ids(quests_of(&target.root, "pmc"), &known_q);
        let scav_quests = modded_quest_ids(quests_of(&target.root, "scav"), &known_q);
        let quest_ids: Vec<String> = pmc_quests.iter().chain(scav_quests.iter()).cloned().collect();

        let builds = target
            .root
            .get("userbuilds")
            .map(|ub| modded_build_ids(ub, &known))
            .unwrap_or_default();

        for id in item_ids.iter().take(4) {
            if let Some(it) = pmc_items.iter().chain(scav_items.iter()).find(|it| {
                it.get("_id").and_then(|v| v.as_str()) == Some(id.as_str())
            }) {
                let tpl = it.get("_tpl").and_then(|v| v.as_str()).unwrap_or("?");
                samples.push(format!("{} · {}", tpl, id));
            }
        }
        for qid in quest_ids.iter().take(4) {
            samples.push(qid.clone());
        }
        for (kind, id) in builds.iter().take(2) {
            samples.push(format!("{} · {}", kind, id));
        }
        samples.truncate(8);

        let items = removed_items.len();
        let encyclopedia = enc_modded.len();
        let quests = quest_ids.len();
        let builds = builds.len();
        Ok(ModCleanData {
            items,
            encyclopedia,
            quests,
            builds,
            total: items + encyclopedia + quests + builds,
            samples,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 清理：四类全部移除，返回删除条数与备份路径
#[tauri::command]
pub async fn mods_clean(app: AppHandle) -> Result<ModCleanResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut target = profile_target(&app)?;
        let known = known_tpls(&app)?;
        let known_q = known_quests(&app)?;

        // 1) 背包物品：先收集 PMC / Scav 的 mod 物品 id，再连带子件一起删
        let pmc_items: Vec<Value> = inv_items_of(&target.root, "pmc").to_vec();
        let scav_items: Vec<Value> = inv_items_of(&target.root, "scav").to_vec();
        let mut roots = modded_item_ids(&pmc_items, &known);
        roots.extend(modded_item_ids(&scav_items, &known));
        let mut drop_ids: HashSet<String> = crate::stash::collect_subtree(&roots, &pmc_items);
        drop_ids.extend(crate::stash::collect_subtree(&roots, &scav_items));

        // 2) Encyclopedia 里 tpl 缺失的键
        let enc_drop: Vec<String> = target
            .root
            .pointer("/characters/pmc/Encyclopedia")
            .and_then(|v| v.as_object())
            .map(|m| m.keys().filter(|k| !known.contains(*k)).cloned().collect())
            .unwrap_or_default();

        // 3) 任务里 qid 缺失的条目
        let quest_drop: HashSet<String> = modded_quest_ids(quests_of(&target.root, "pmc"), &known_q)
            .into_iter()
            .chain(modded_quest_ids(quests_of(&target.root, "scav"), &known_q))
            .collect();

        // 4) 含缺失物品的预设
        let build_drop: Vec<(String, String)> = target
            .root
            .get("userbuilds")
            .map(|ub| modded_build_ids(ub, &known))
            .unwrap_or_default();

        let removed = drop_ids.len() + enc_drop.len() + quest_drop.len() + build_drop.len();
        if removed == 0 {
            // 没有可清理的东西：不写盘、不产生备份
            return Ok(ModCleanResult {
                removed: 0,
                backup: None,
            });
        }

        // 落盘前先改内存里的 JSON（借用分离：先算完再改）
        if let Some(chars) = target.root.get_mut("characters").and_then(|v| v.as_object_mut()) {
            for who in ["pmc", "scav"] {
                if let Some(ch) = chars.get_mut(who).and_then(|v| v.as_object_mut()) {
                    if let Some(items) = ch
                        .get_mut("Inventory")
                        .and_then(|v| v.get_mut("items"))
                        .and_then(|v| v.as_array_mut())
                    {
                        items.retain(|it| {
                            it.get("_id")
                                .and_then(|v| v.as_str())
                                .map(|id| !drop_ids.contains(id))
                                .unwrap_or(true)
                        });
                    }
                    if let Some(qs) = ch.get_mut("Quests").and_then(|v| v.as_array_mut()) {
                        qs.retain(|q| {
                            q.get("qid")
                                .and_then(|v| v.as_str())
                                .map(|qid| !quest_drop.contains(qid))
                                .unwrap_or(true)
                        });
                    }
                }
            }
            if let Some(enc) = chars
                .get_mut("pmc")
                .and_then(|v| v.get_mut("Encyclopedia"))
                .and_then(|v| v.as_object_mut())
            {
                for key in &enc_drop {
                    enc.remove(key);
                }
            }
        }

        if !build_drop.is_empty() {
            if let Some(ub) = target.root.get_mut("userbuilds").and_then(|v| v.as_object_mut()) {
                for (kind, id) in &build_drop {
                    if let Some(list) = ub.get_mut(kind).and_then(|v| v.as_array_mut()) {
                        list.retain(|b| b.get("_id").and_then(|v| v.as_str()) != Some(id.as_str()));
                    }
                }
            }
        }

        let backup = backup_before_write(&app, &target.path)?;
        let text = serde_json::to_string(&target.root).map_err(|e| e.to_string())?;
        fs::write(&target.path, text).map_err(|e| e.to_string())?;
        Ok(ModCleanResult {
            removed,
            backup: backup.map(|p| p.to_string_lossy().to_string()),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
