//! 技能 / 武器熟练页（06-skills）数据服务 —— 读全量 / 写回。
//! 结构对照参考编辑器（CharacterSkills.cs / CharacterSkill.cs / Mastering.cs）：
//! - 档案侧：characters.{pmc|scav}.Skills.{Common|Mastering} 数组，行含 Id/Progress
//!   （LastAccess / PointsEarnedDuringSession 不展示、写回不触碰）；
//! - 上限：普通技能统一 5100（参考 DefaultValues.CommonSkillMaxValue，51 级 × 100 点）；
//!   武器熟练 = globals config.mastering 对应项 Level2 + Level3（查无同回退 5100）；
//! - 名称：locales/global/{lang}.json 直接取 {Id} 键（CharacterSkill.LocalizedName
//!   口径）；武器熟练无直接键时回退拼接该武器模板名（MasteringLocalizedName 口径），
//!   再缺失原样显示 Id；
//! - bot 开头的技能（BotReload / BotSound）原样读出，前端「全部拉满」跳过；
//! - 写回铁律：先备份，后按 Id 定点替换 Progress（查无该 Id 则跳过不新建）。

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::AppHandle;

use crate::backup::backup_before_write;
use crate::character::database_dir;
use crate::profile::{is_safe_id, profiles_dir, SaveResult};
use crate::settings;

/// 普通技能统一上限（参考 DefaultValues.CommonSkillMaxValue：51 级 × 100 点）
const COMMON_SKILL_MAX: i64 = 5100;

// ———————— 类型 ————————

/// 单行技能 / 武器熟练（name 已本地化；cap 供前端滑杆 max 与「全部拉满」）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillRow {
    pub id: String,
    pub name: String,
    pub progress: i64,
    pub cap: i64,
    /// bot 开头的机器人内部技能（拉满无意义，前端跳过）
    pub bot: bool,
}

/// 单侧技能组（Common = 普通技能，Mastering = 武器熟练）
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillSideData {
    pub common: Vec<SkillRow>,
    pub mastering: Vec<SkillRow>,
}

/// 技能页全量数据：PMC / Scav 双侧
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillsData {
    pub pmc: SkillSideData,
    pub scav: SkillSideData,
}

/// 保存载荷行（side: pmc/scav；group: Common/Mastering）
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillsSaveRow {
    pub side: String,
    pub group: String,
    pub id: String,
    pub progress: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillsSaveData {
    pub rows: Vec<SkillsSaveRow>,
}

// ———————— 读侧 helpers ————————

/// 武器熟练条目：上限（Level2 + Level3）与模板 id 列表（名称回退拼接用）
#[derive(Debug, Clone)]
struct MasteringEntry {
    cap: i64,
    tpls: Vec<String>,
}

/// 武器熟练表：globals config.mastering 的 Name → { cap, tpls }
fn mastering_table(app: &AppHandle) -> HashMap<String, MasteringEntry> {
    let mut table = HashMap::new();
    let Some(db) = database_dir(app) else {
        return table;
    };
    let Ok(txt) = fs::read_to_string(Path::new(&db).join("globals.json")) else {
        return table;
    };
    let Ok(v) = serde_json::from_str::<Value>(&txt) else {
        return table;
    };
    if let Some(arr) = v.pointer("/config/mastering").and_then(|m| m.as_array()) {
        for m in arr {
            let Some(name) = m.get("Name").and_then(|n| n.as_str()) else {
                continue;
            };
            let l2 = m.get("Level2").and_then(|x| x.as_f64()).unwrap_or(0.0) as i64;
            let l3 = m.get("Level3").and_then(|x| x.as_f64()).unwrap_or(0.0) as i64;
            let tpls = m
                .get("Templates")
                .and_then(|t| t.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            table.insert(
                name.to_string(),
                MasteringEntry {
                    cap: l2 + l3,
                    tpls,
                },
            );
        }
    }
    table
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

/// 技能名（CharacterSkill.LocalizedName 口径）：locale 直接取 {Id} 键；
/// 武器熟练回退拼接模板名（{tpl} Name 键、顿号连接）；再缺失原样 Id
fn skill_name(names: Option<&Value>, id: &str, tpls: Option<&[String]>) -> String {
    if let Some(v) = names.and_then(|n| n.get(id)).and_then(|v| v.as_str()) {
        return v.to_string();
    }
    if let Some(tpls) = tpls {
        let parts: Vec<String> = tpls
            .iter()
            .filter_map(|tpl| {
                names
                    .and_then(|n| n.get(format!("{} Name", tpl)))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .collect();
        if !parts.is_empty() {
            return parts.join("、");
        }
    }
    id.to_string()
}

/// bot 内部技能判定：Id 以 bot 开头（不区分大小写）
fn is_bot(id: &str) -> bool {
    id.to_lowercase().starts_with("bot")
}

/// 读一组技能行（group: "Common" / "Mastering"）。is_mastering 决定上限口径与
/// 名称回退策略；行结构异常（非对象 / 缺 Id）跳过
fn read_group(
    side: Option<&Value>,
    group: &str,
    is_mastering: bool,
    mtable: &HashMap<String, MasteringEntry>,
    names: Option<&Value>,
) -> Vec<SkillRow> {
    side.and_then(|s| s.get("Skills"))
        .and_then(|sk| sk.get(group))
        .and_then(|g| g.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|row| {
                    let id = row.get("Id").and_then(|v| v.as_str())?;
                    let entry = mtable.get(id);
                    let progress =
                        row.get("Progress").and_then(|v| v.as_f64()).unwrap_or(0.0) as i64;
                    Some(SkillRow {
                        id: id.to_string(),
                        name: skill_name(
                            names,
                            id,
                            if is_mastering {
                                entry.map(|e| e.tpls.as_slice())
                            } else {
                                None
                            },
                        ),
                        progress,
                        cap: if is_mastering {
                            entry.map(|e| e.cap).unwrap_or(COMMON_SKILL_MAX)
                        } else {
                            COMMON_SKILL_MAX
                        },
                        bot: is_bot(id),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

// ———————— 自动补全（设置页「自动补全」开关消费端） ————————

/// 写盘/读视图前补全缺失技能，语义对齐参考 Profile.Load：
/// - Scav 普通技能（auto_add_scav_skills）：PMC/Scav Common 都存在且 Scav Common
///   为空数组时，按 PMC 行集复制新行（只取 Id，Progress 清零——不是复制进度）；
/// - 武器熟练（auto_add_masterings）：PMC/Scav 双侧 Mastering 数组存在时，对照
///   globals config.mastering 补缺失行 {Id, Progress: 0}（空数组也照补）；
/// 两个开关都关时直接返回，档案零扰动。
fn auto_add_missing(
    app: &AppHandle,
    mtable: &HashMap<String, MasteringEntry>,
    raw: &mut Value,
) {
    let s = settings::get(app);
    if !s.auto_add_scav_skills && !s.auto_add_masterings {
        return;
    }

    if s.auto_add_scav_skills {
        let pmc_common = raw.pointer("/characters/pmc/Skills/Common");
        let scav_common = raw.pointer("/characters/scav/Skills/Common");
        if let (Some(pmc), Some(scav)) = (pmc_common, scav_common) {
            // 参考条件：两侧都非 null，且 Scav 侧 Count == 0
            if scav.as_array().is_some_and(|a| a.is_empty()) {
                let rows: Vec<Value> = pmc
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|r| r.get("Id").and_then(|v| v.as_str()))
                            .map(|id| json!({ "Id": id, "Progress": 0 }))
                            .collect()
                    })
                    .unwrap_or_default();
                if let Some(scav_arr) = raw
                    .pointer_mut("/characters/scav/Skills/Common")
                    .and_then(|v| v.as_array_mut())
                {
                    *scav_arr = rows;
                }
            }
        }
    }

    if s.auto_add_masterings {
        let mut names: Vec<String> = mtable.keys().cloned().collect();
        names.sort();
        for side in ["pmc", "scav"] {
            let Some(arr) = raw
                .pointer_mut(&format!("/characters/{}/Skills/Mastering", side))
                .and_then(|v| v.as_array_mut())
            else {
                continue;
            };
            for name in &names {
                let exists = arr
                    .iter()
                    .any(|r| r.get("Id").and_then(|v| v.as_str()) == Some(name.as_str()));
                if !exists {
                    arr.push(json!({ "Id": name, "Progress": 0 }));
                }
            }
        }
    }
}

// ———————— 命令实现 ————————

/// 技能页全量数据；档案缺失或不可读时为 None
pub fn load_skills(app: &AppHandle, id: &str) -> Option<SkillsData> {
    if !is_safe_id(id) {
        return None;
    }
    let dir = profiles_dir(app)?;
    let mut raw: Value =
        serde_json::from_str(&fs::read_to_string(Path::new(&dir).join(format!("{}.json", id))).ok()?)
            .ok()?;
    let mtable = mastering_table(app);
    // 设置开关开启时在内存里补全缺失行（不改文件；用户保存时才随 save_skills 落盘）
    auto_add_missing(app, &mtable, &mut raw);
    let lang = raw
        .pointer("/info/language")
        .and_then(|v| v.as_str())
        .unwrap_or("en");
    let names = locales_table(app, lang);

    let pmc = raw.pointer("/characters/pmc");
    let scav = raw.pointer("/characters/scav");
    Some(SkillsData {
        pmc: SkillSideData {
            common: read_group(pmc, "Common", false, &mtable, names.as_ref()),
            mastering: read_group(pmc, "Mastering", true, &mtable, names.as_ref()),
        },
        scav: SkillSideData {
            common: read_group(scav, "Common", false, &mtable, names.as_ref()),
            mastering: read_group(scav, "Mastering", true, &mtable, names.as_ref()),
        },
    })
}

// ———————— 写回 ————————

/// 技能页写回：先强制备份（领域铁律），再按 side/group/Id 定点替换 Progress
pub fn save_skills(app: &AppHandle, id: &str, data: &SkillsSaveData) -> Result<SaveResult, String> {
    if !is_safe_id(id) {
        return Err(format!("Invalid profile id: {}", id));
    }
    let dir = profiles_dir(app).ok_or_else(|| "未选择 SPT 服务器".to_string())?;
    let path = Path::new(&dir).join(format!("{}.json", id));
    let txt = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut raw: Value = serde_json::from_str(&txt).map_err(|e| e.to_string())?;
    let backup_path = backup_before_write(app, &path)?;

    let mtable = mastering_table(app);
    // 设置开关开启时先补全缺失行再定点替换——否则用户对补全行的修改会被
    // 「查无该 Id：跳过」静默丢弃，开关沦为假开关
    auto_add_missing(app, &mtable, &mut raw);
    for row in &data.rows {
        // side / group 白名单校验，防止路径注入
        if !matches!(row.side.as_str(), "pmc" | "scav") {
            continue;
        }
        if !matches!(row.group.as_str(), "Common" | "Mastering") {
            continue;
        }
        let is_mastering = row.group == "Mastering";
        let cap = if is_mastering {
            mtable.get(&row.id).map(|e| e.cap).unwrap_or(COMMON_SKILL_MAX)
        } else {
            COMMON_SKILL_MAX
        };
        let arr = raw
            .pointer_mut(&format!(
                "/characters/{}/Skills/{}",
                row.side, row.group
            ))
            .and_then(|v| v.as_array_mut());
        let Some(arr) = arr else { continue };
        if let Some(item) = arr
            .iter_mut()
            .find(|it| it.get("Id").and_then(|v| v.as_str()) == Some(row.id.as_str()))
        {
            // 数值口径：强转整数并夹在 0..cap（参考 CharacterSkill.Progress setter）
            item["Progress"] = json!(row.progress.max(0).min(cap));
        }
        // 查无该 Id：跳过不新建（与口袋/商人写回同口径）
    }

    let out = serde_json::to_string_pretty(&raw).map_err(|e| e.to_string())?;
    fs::write(&path, out).map_err(|e| e.to_string())?;
    Ok(SaveResult {
        backup_path: backup_path.map(|p| p.to_string_lossy().to_string()),
    })
}
