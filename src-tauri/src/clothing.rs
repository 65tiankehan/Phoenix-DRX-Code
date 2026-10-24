//! 形象与服装（稿图 31，0.2.0 第 2 段）——角色页「形象服装」tab 的数据层。
//! 数据通路（参考 SkiTles55 4.1.1 AppData.LoadTraderSuits / TraderSuit.Boughted / CharacterCustomization）：
//!   - 套装目录：<database>/traders/*/suits.json 的 { _id, suiteId }，按 suiteId 去重
//!   - 已拥有判定：profile.customisationUnlocks 含 { id=suiteId, source=unlockedInGame, type=suite }
//!   - 解锁/上锁 = 增删该数组条目（对齐参考 CustomisationUnlock 序列化三字段）
//!   - 头型/语音：character.customization.{head,voice}；候选来自 database/bots/types 的 appearance.head/voice
//!     的 appearance.heads / appearance.voices 键（名字查 locales/global/en.json）
//! 写入前一律走 backup_before_write（章程 §四 铁律）。

use serde::Serialize;
use serde_json::{json, Map, Value};
use std::fs;
use tauri::AppHandle;

use crate::backup::backup_before_write;
use crate::character::database_dir;
use crate::profile::get_current_profile_id;
use crate::profile::profiles_dir;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhxSuit {
    pub suite_id: String,
    pub trader_id: String,
    pub name: String,
    pub owned: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhxLookOption {
    pub id: String,
    pub name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClothingData {
    pub suits: Vec<PhxSuit>,
    pub heads: Vec<PhxLookOption>,
    pub voices: Vec<PhxLookOption>,
    pub current_head: String,
    pub current_voice: String,
    pub owned_count: usize,
}

/// locales/global/en.json 的「{key} Name」，缺失兜底 key 原文（对齐 character.rs 名字表惯例）
fn localized_name(names: &Map<String, Value>, key: &str) -> String {
    // 头型/语音/套装在 locales 里是「id 直键」，物品名才是「{tpl} Name」——直键优先，Name 兜底
    names
        .get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            names
                .get(&format!("{} Name", key))
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.to_string())
                .unwrap_or_else(|| key.to_string())
        })
}

fn profile_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let id = get_current_profile_id(app).ok_or("未选择存档")?;
    let dir = profiles_dir(app).ok_or("未选择 SPT 服务器")?;
    Ok(std::path::Path::new(&dir).join(format!("{}.json", id)))
}

fn read_profile(app: &AppHandle) -> Result<Value, String> {
    let path = profile_path(app)?;
    serde_json::from_str(&fs::read_to_string(&path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

fn write_profile(app: &AppHandle, root: &Value) -> Result<Option<String>, String> {
    let path = profile_path(app)?;
    let backup = backup_before_write(app, &path)?;
    let text = serde_json::to_string_pretty(root).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(backup.map(|p| p.to_string_lossy().to_string()))
}

/// 读取：套装目录 + 已拥有 + 头型/语音候选与当前值
#[tauri::command]
pub async fn clothing_load(app: AppHandle) -> Result<ClothingData, String> {
    tauri::async_runtime::spawn_blocking(move || clothing_load_sync(&app))
        .await
        .map_err(|e| e.to_string())?
}

fn clothing_load_sync(app: &AppHandle) -> Result<ClothingData, String> {
    let root = read_profile(app)?;
    let owned: Vec<String> = root
        .get("customisationUnlocks")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|e| {
                    e.get("type").and_then(|v| v.as_str()) == Some("suite")
                        && e.get("source").and_then(|v| v.as_str()) == Some("unlockedInGame")
                })
                .filter_map(|e| e.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let db = database_dir(app).ok_or("未找到服务器数据库目录")?;
    let server_root = format!(
        "{}\\SPT\\SPT_Data\\database",
        crate::server::current_server(app).ok_or("未选择 SPT 服务器")?.path.trim_end_matches(['\\', '/'])
    );
    let names: Map<String, Value> = fs::read_to_string(
        std::path::Path::new(&db).join("locales").join("global").join("en.json"),
    )
    .ok()
    .and_then(|t| serde_json::from_str(&t).ok())
    .unwrap_or_default();

    // 套装目录：traders/*/suits.json，按 suiteId 去重（后写跳过，对齐参考实现）
    let mut suits: Vec<PhxSuit> = Vec::new();
    if let Ok(entries) = fs::read_dir(std::path::Path::new(&db).join("traders")) {
        let mut dirs: Vec<_> = entries.flatten().collect();
        dirs.sort_by_key(|e| e.file_name());
        for trader_dir in dirs {
            let suits_path = trader_dir.path().join("suits.json");
            let Ok(text) = fs::read_to_string(&suits_path) else {
                continue;
            };
            let Ok(list) = serde_json::from_str::<Vec<Value>>(&text) else {
                continue;
            };
            let trader_id = trader_dir.file_name().to_string_lossy().to_string();
            for s in list {
                let Some(suite_id) = s.get("suiteId").and_then(|v| v.as_str()) else {
                    continue;
                };
                if suits.iter().any(|x| x.suite_id == suite_id) {
                    continue;
                }
                suits.push(PhxSuit {
                    suite_id: suite_id.to_string(),
                    trader_id: trader_id.clone(),
                    name: localized_name(&names, suite_id),
                    owned: owned.iter().any(|o| o == suite_id),
                });
            }
        }
    }
    suits.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    // 头型/语音候选：bots/*.json 的 appearance.heads / appearance.voices 键
    let (heads, voices) = load_look_options(&server_root, &names);

    let cust = root.pointer("/characters/pmc/Customization").cloned().unwrap_or(Value::Null);
    Ok(ClothingData {
        owned_count: owned.len(),
        current_head: cust.get("Head").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        current_voice: cust.get("Voice").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        suits,
        heads,
        voices,
    })
}

fn load_look_options(server_root: &str, names: &Map<String, Value>) -> (Vec<PhxLookOption>, Vec<PhxLookOption>) {
    let mut heads: Vec<PhxLookOption> = Vec::new();
    let mut voices: Vec<PhxLookOption> = Vec::new();
    let push_keys = |file_json: &Value, slot: &str, target: &mut Vec<PhxLookOption>| {
        let Some(map) = file_json.pointer(&format!("/appearance/{}", slot)).and_then(|v| v.as_object()) else {
            return;
        };
        for key in map.keys() {
            if !target.iter().any(|o| o.id == *key) {
                target.push(PhxLookOption {
                    id: key.clone(),
                    name: localized_name(names, key),
                });
            }
        }
    };
    // bots 目录：database/bots/types（参考 DefaultValues）；appearance 的 head/voice
    // 真实数据为单数键（复数 heads/voices 为参考仓模型映射，一并兼容）
    if let Ok(files) = fs::read_dir(std::path::Path::new(server_root).join("bots").join("types")) {
        let mut paths: Vec<_> = files.flatten().map(|e| e.path()).collect();
        paths.sort();
        for p in paths {
            if let Ok(text) = fs::read_to_string(&p) {
                if let Ok(v) = serde_json::from_str::<Value>(&text) {
                    push_keys(&v, "head", &mut heads);
                    push_keys(&v, "heads", &mut heads);
                    push_keys(&v, "voice", &mut voices);
                    push_keys(&v, "voices", &mut voices);
                }
            }
        }
    }
    heads.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    voices.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    (heads, voices)
}

/// 保存：owned 集合全量重建 customisationUnlocks 的 suit 条目（非 suit 条目原样保留）+ head/voice
#[tauri::command]
pub async fn clothing_save(
    app: AppHandle,
    owned_suite_ids: Vec<String>,
    head: String,
    voice: String,
) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut root = read_profile(&app)?;
        let mut unlocks = root
            .get("customisationUnlocks")
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        // 保留全部非 suit 条目（其他类型的解锁不能误删）
        unlocks.retain(|e| {
            !(e.get("type").and_then(|v| v.as_str()) == Some("suite")
                && e.get("source").and_then(|v| v.as_str()) == Some("unlockedInGame"))
        });
        for id in &owned_suite_ids {
            unlocks.push(json!({ "id": id, "source": "unlockedInGame", "type": "suite" }));
        }
        if let Some(obj) = root.as_object_mut() {
            obj.insert("customisationUnlocks".into(), Value::Array(unlocks));
        } else {
            return Err("存档根节点不是对象".into());
        }

        if !head.is_empty() || !voice.is_empty() {
            let Some(cust) = root
                .pointer_mut("/characters/pmc/Customization")
                .and_then(|v| v.as_object_mut())
            else {
                return Err("存档缺少 characters.pmc.Customization 节点".into());
            };
            if !head.is_empty() {
                cust.insert("Head".into(), json!(head));
            }
            if !voice.is_empty() {
                cust.insert("Voice".into(), json!(voice));
            }
        }
        write_profile(&app, &root)
    })
    .await
    .map_err(|e| e.to_string())?
}
