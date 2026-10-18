//! 健康页（07-health）数据服务 —— 读全量 / 写回。
//! 结构对照参考编辑器（CharacterInfo.cs 健康区 + CharacterMetric setter）：
//! - 档案侧：characters.{pmc|scav}.Health——七部位在 BodyParts.{部位}.Health.{Current,
//!   Maximum}，水分 / 能量直挂 Health.{Hydration|Energy}.{Current, Maximum}；
//! - 联动口径（参考 CharacterMetric.Current setter）：当前值 > 上限时上限自动抬高
//!   到当前值；上限可单独下调、不反向联动当前值；两侧数值最低 0；
//! - 写回铁律：先备份，后定点替换；只触碰 Health 子树，不动档案其他字段。

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::AppHandle;

use crate::backup::backup_before_write;
use crate::profile::{is_safe_id, profiles_dir, SaveResult};

// ———————— 类型 ————————

/// 单项指标（当前值 / 上限值）
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthVital {
    pub current: i64,
    pub maximum: i64,
}

/// 单侧九项指标：七部位 + 水分 + 能量
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthSide {
    pub head: HealthVital,
    pub chest: HealthVital,
    pub stomach: HealthVital,
    pub left_arm: HealthVital,
    pub right_arm: HealthVital,
    pub left_leg: HealthVital,
    pub right_leg: HealthVital,
    pub hydration: HealthVital,
    pub energy: HealthVital,
}

/// 健康页全量数据：PMC / Scav 双侧
#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthData {
    pub pmc: HealthSide,
    pub scav: HealthSide,
}

/// 保存载荷（双侧全量）
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthSaveData {
    pub pmc: HealthSide,
    pub scav: HealthSide,
}

// ———————— 读侧 helpers ————————

/// 部位体征：{part}.Health.{Current,Maximum}；缺失按 0
fn vital_in(part: Option<&Value>) -> HealthVital {
    let hp = part.and_then(|p| p.get("Health"));
    HealthVital {
        current: hp
            .and_then(|h| h.get("Current"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as i64,
        maximum: hp
            .and_then(|h| h.get("Maximum"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as i64,
    }
}

/// 直挂体征：{stat}.{Current,Maximum}（Hydration / Energy 无内层 Health）
fn vital_direct(stat: Option<&Value>) -> HealthVital {
    HealthVital {
        current: stat
            .and_then(|s| s.get("Current"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as i64,
        maximum: stat
            .and_then(|s| s.get("Maximum"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as i64,
    }
}

fn read_side(side: Option<&Value>) -> HealthSide {
    let health = side.and_then(|s| s.get("Health"));
    let body = health.and_then(|h| h.get("BodyParts"));
    HealthSide {
        head: vital_in(body.and_then(|b| b.get("Head"))),
        chest: vital_in(body.and_then(|b| b.get("Chest"))),
        stomach: vital_in(body.and_then(|b| b.get("Stomach"))),
        left_arm: vital_in(body.and_then(|b| b.get("LeftArm"))),
        right_arm: vital_in(body.and_then(|b| b.get("RightArm"))),
        left_leg: vital_in(body.and_then(|b| b.get("LeftLeg"))),
        right_leg: vital_in(body.and_then(|b| b.get("RightLeg"))),
        hydration: vital_direct(health.and_then(|h| h.get("Hydration"))),
        energy: vital_direct(health.and_then(|h| h.get("Energy"))),
    }
}

// ———————— 命令实现 ————————

/// 健康页全量数据；档案缺失或不可读时为 None
pub fn load_health(app: &AppHandle, id: &str) -> Option<HealthData> {
    if !is_safe_id(id) {
        return None;
    }
    let dir = profiles_dir(app)?;
    let raw: Value =
        serde_json::from_str(&fs::read_to_string(Path::new(&dir).join(format!("{}.json", id))).ok()?)
            .ok()?;
    Some(HealthData {
        pmc: read_side(raw.pointer("/characters/pmc")),
        scav: read_side(raw.pointer("/characters/scav")),
    })
}

// ———————— 写回 ————————

/// 单侧定点替换：七部位 + 水分 / 能量的 Current / Maximum；
/// 联动规则在写入侧兜底——当前 > 上限时上限抬高到当前值（CharacterMetric.Current setter）
fn write_side(side: Option<&mut Value>, data: &HealthSide) {
    let Some(side) = side else { return };
    let Some(health) = side.get_mut("Health") else { return };
    for (key, vital) in [
        ("Head", data.head),
        ("Chest", data.chest),
        ("Stomach", data.stomach),
        ("LeftArm", data.left_arm),
        ("RightArm", data.right_arm),
        ("LeftLeg", data.left_leg),
        ("RightLeg", data.right_leg),
    ] {
        if let Some(hp) = health
            .pointer_mut(&format!("/BodyParts/{key}/Health"))
        {
            let maximum = vital.maximum.max(0).max(vital.current.max(0));
            hp["Current"] = json!(vital.current.max(0));
            hp["Maximum"] = json!(maximum);
        }
    }
    for (key, vital) in [("Hydration", data.hydration), ("Energy", data.energy)] {
        if let Some(stat) = health.get_mut(key) {
            let maximum = vital.maximum.max(0).max(vital.current.max(0));
            stat["Current"] = json!(vital.current.max(0));
            stat["Maximum"] = json!(maximum);
        }
    }
}

/// 健康页写回：先强制备份（领域铁律），再双侧定点替换后整体序列化写回
pub fn save_health(app: &AppHandle, id: &str, data: &HealthSaveData) -> Result<SaveResult, String> {
    if !is_safe_id(id) {
        return Err(format!("Invalid profile id: {}", id));
    }
    let dir = profiles_dir(app).ok_or_else(|| "未选择 SPT 服务器".to_string())?;
    let path = Path::new(&dir).join(format!("{}.json", id));
    let txt = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut raw: Value = serde_json::from_str(&txt).map_err(|e| e.to_string())?;
    let backup_path = backup_before_write(app, &path)?;

    write_side(raw.pointer_mut("/characters/pmc"), &data.pmc);
    write_side(raw.pointer_mut("/characters/scav"), &data.scav);

    let out = serde_json::to_string_pretty(&raw).map_err(|e| e.to_string())?;
    fs::write(&path, out).map_err(|e| e.to_string())?;
    Ok(SaveResult {
        backup_path: backup_path.map(|p| p.to_string_lossy().to_string()),
    })
}
