//! 商人页（04-traders）数据服务 —— 读全量 / 写回。
//!
//! 数据口径（对照参考编辑器 ProfileSaver.cs / CharacterTraderStanding.cs /
//! CharacterTraderStandingExtended.cs / TraderBase.cs）：
//! - 档案 TradersInfo：逐商人四字段 loyaltyLevel(int) / salesSum(long) /
//!   standing(float) / unlocked(bool)；
//! - 商人信息 base.json（<database>/traders/<id>/base.json）：仅四字段
//!   _id / avatar / nickname / loyaltyLevels，其中 loyaltyLevels 为数组，
//!   段数即商人等级上限；
//! - 写回：只写四字段到原 token（档案里没有的商人条目不新建）；
//!   standing 四舍五入 2 位；RagfairInfo.rating 同 round 2；
//! - Fence 同步：PMC 写 Fence（579dc571d53a0658a154fbec）好感时，
//!   scav.TradersInfo 同 ID 仅同步 standing（不同步等级/销售额），
//!   对齐参考 CharacterTraderStandingExtended.Standing setter；
//! - 跳蚤市场仅编辑 rating 一项。
//! 写回铁律：先备份，后定点替换。

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::AppHandle;

use crate::backup::backup_before_write;
use crate::character::database_dir;
use crate::profile::{is_safe_id, profiles_dir, SaveResult};

/// Fence 商人 ID（SPT 源码 Traders 枚举实证）
const FENCE_ID: &str = "579dc571d53a0658a154fbec";

// ———————— 类型 ————————

/// base.json 单等级段下限（前端联动口径：升级补下限 / 手改反推等级）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraderLevel {
    pub min_sales_sum: f64,
    pub min_standing: f64,
}

/// 商人行：档案四字段 + base.json 派生信息
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraderRow {
    pub id: String,
    /// 商人原名（base.json.nickname）；缺失为空串，前端回退本地名
    pub nickname: String,
    pub loyalty_level: i64,
    pub sales_sum: i64,
    pub standing: f64,
    pub unlocked: bool,
    /// 等级上限 = max(base.loyaltyLevels 段数, 当前等级, 1)
    pub max_level: i64,
    /// 各等级段下限（base.json 缺失为空，前端联动退化为保持当前值）
    pub levels: Vec<TraderLevel>,
}

/// 跳蚤市场行（仅评分）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RagfairRow {
    pub rating: f64,
}

/// 商人页全量数据
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TradersData {
    pub traders: Vec<TraderRow>,
    pub ragfair: RagfairRow,
}

/// 商人写回行（四字段）
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraderSaveRow {
    pub id: String,
    pub loyalty_level: i64,
    pub sales_sum: i64,
    pub standing: f64,
    pub unlocked: bool,
}

/// 商人页写回载荷
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TradersSaveData {
    pub traders: Vec<TraderSaveRow>,
    pub ragfair: f64,
}

// ———————— helpers ————————

/// 四舍五入 2 位（对齐参考 Math.Round(x, 2)）
fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// 商人基础信息：<database>/traders/<id>/base.json 的 nickname、等级段数
/// 与各段下限（前端联动口径）；缺失回退空名 + 当前等级 + 空段表。
fn base_info(app: &AppHandle, trader_id: &str, current_level: i64) -> (String, i64, Vec<TraderLevel>) {
    let Some(dir) = database_dir(app) else {
        return (String::new(), current_level.max(1), Vec::new());
    };
    let path = Path::new(&dir)
        .join("traders")
        .join(trader_id)
        .join("base.json");
    let Ok(txt) = fs::read_to_string(path) else {
        return (String::new(), current_level.max(1), Vec::new());
    };
    let Ok(base) = serde_json::from_str::<Value>(&txt) else {
        return (String::new(), current_level.max(1), Vec::new());
    };
    let nickname = base
        .get("nickname")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let segs = base
        .get("loyaltyLevels")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let max_level = (segs.len() as i64).max(current_level).max(1);
    let levels = segs
        .iter()
        .map(|seg| TraderLevel {
            min_sales_sum: seg
                .get("minSalesSum")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
            min_standing: seg
                .get("minStanding")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
        })
        .collect();
    (nickname, max_level, levels)
}

// ———————— 读 / 写 ————————

/// 商人页全量数据（PMC 侧 + 跳蚤评分）；档案缺失或不可读时为 None
pub fn load_traders(app: &AppHandle, id: &str) -> Option<TradersData> {
    if !is_safe_id(id) {
        return None;
    }
    let dir = profiles_dir(app)?;
    let raw: Value =
        serde_json::from_str(&fs::read_to_string(Path::new(&dir).join(format!("{}.json", id))).ok()?)
            .ok()?;
    let pmc = raw.pointer("/characters/pmc");

    let mut traders = Vec::new();
    if let Some(map) = pmc.and_then(|p| p.get("TradersInfo")).and_then(|t| t.as_object()) {
        for (trader_id, info) in map {
            let level = info
                .get("loyaltyLevel")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as i64;
            let (nickname, max_level, levels) = base_info(app, trader_id, level);
            traders.push(TraderRow {
                id: trader_id.clone(),
                nickname,
                loyalty_level: level,
                sales_sum: info
                    .get("salesSum")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as i64,
                standing: info.get("standing").and_then(|v| v.as_f64()).unwrap_or(0.0),
                unlocked: info
                    .get("unlocked")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                max_level,
                levels,
            });
        }
    }

    let ragfair_rating = pmc
        .and_then(|p| p.pointer("/RagfairInfo/rating"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    Some(TradersData {
        traders,
        ragfair: RagfairRow {
            rating: ragfair_rating,
        },
    })
}

/// 商人页写回：先备份，后定点替换。
/// 只写档案已有的商人 token；Fence 同步 scav 同 ID 的 standing。
pub fn save_traders(app: &AppHandle, id: &str, data: &TradersSaveData) -> Result<SaveResult, String> {
    if !is_safe_id(id) {
        return Err(format!("Invalid profile id: {}", id));
    }
    let dir = profiles_dir(app).ok_or_else(|| "未选择 SPT 服务器".to_string())?;
    let path = Path::new(&dir).join(format!("{}.json", id));
    let txt = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut raw: Value = serde_json::from_str(&txt).map_err(|e| e.to_string())?;
    let backup_path = backup_before_write(app, &path)?;

    for row in &data.traders {
        // PMC 侧：仅写已有 token 的四字段
        let Some(trader) = raw.pointer_mut(&format!("/characters/pmc/TradersInfo/{}", row.id))
        else {
            continue;
        };
        if let Some(v) = trader.get_mut("loyaltyLevel") {
            *v = Value::from(row.loyalty_level);
        }
        if let Some(v) = trader.get_mut("salesSum") {
            *v = Value::from(row.sales_sum);
        }
        if let Some(v) = trader.get_mut("standing") {
            *v = Value::from(round2(row.standing));
        }
        if let Some(v) = trader.get_mut("unlocked") {
            *v = Value::from(row.unlocked);
        }

        // Fence 同步：scav.TradersInfo 同 ID 仅同步 standing
        if row.id == FENCE_ID {
            if let Some(scav_trader) =
                raw.pointer_mut(&format!("/characters/scav/TradersInfo/{}", row.id))
            {
                if let Some(v) = scav_trader.get_mut("standing") {
                    *v = Value::from(round2(row.standing));
                }
            }
        }
    }

    // 跳蚤评分：仅写已有 RagfairInfo.rating
    if let Some(rating) = raw.pointer_mut("/characters/pmc/RagfairInfo/rating") {
        *rating = Value::from(round2(data.ragfair));
    }

    let out = serde_json::to_string(&raw).map_err(|e| e.to_string())?;
    fs::write(&path, out).map_err(|e| e.to_string())?;
    Ok(SaveResult {
        backup_path: backup_path.map(|p| p.to_string_lossy().to_string()),
    })
}
