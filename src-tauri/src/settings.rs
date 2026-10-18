//! 应用设置服务 —— 存档编辑行为偏好，持久化在 <appData>/settings.json。
//! 语义对齐稿图 15-settings：「改动即时生效并自动保存」，字段与默认值
//! 以参考编辑器 AppSettings.cs 为准：
//!   autoAddMissingMasterings / autoAddMissingScavSkills 默认 false，
//!   IssuesAction 默认 AlwaysShow（ask）。
//! 音效 / 减少动态效果是渲染层感官偏好（localStorage，感官层基线裁决），不入本文件。

use std::fs;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// 问题处理三态（对齐参考枚举 IssuesAction；文案由渲染层 i18n 映射）。
/// fix / ignore 两态仅由渲染层校验后透传，Rust 侧无消费点，不设常量。
pub const ISSUE_ASK: &str = "ask";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// 自动补全缺失的武器精通/专精等级
    pub auto_add_masterings: bool,
    /// 自动补全缺失的 Scav 技能等级
    pub auto_add_scav_skills: bool,
    /// 问题处理三态：ask / fix / ignore
    pub issue_handling: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_add_masterings: false,
            auto_add_scav_skills: false,
            issue_handling: ISSUE_ASK.to_string(),
        }
    }
}

fn load(app: &AppHandle) -> Settings {
    let Ok(path) = app.path().app_data_dir() else {
        return Settings::default();
    };
    serde_json::from_str(&fs::read_to_string(path.join("settings.json")).unwrap_or_default())
        .unwrap_or_default()
}

fn save(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("settings.json");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

/// 读取全部设置
pub fn get(app: &AppHandle) -> Settings {
    load(app)
}

/// 整体保存（改动即时生效并自动保存）
pub fn set(app: &AppHandle, settings: Settings) -> Result<(), String> {
    save(app, &settings)
}

/// 打开数据文件夹（Windows 资源管理器，照托盘 open_profiles_dir 模式）
pub fn open_data_dir(app: &AppHandle) -> Result<(), String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::process::Command::new("explorer")
        .arg(dir)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 重置设置：删除 settings.json，下次读取回落默认值
pub fn reset(app: &AppHandle) -> Result<(), String> {
    let path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("settings.json");
    if path.exists() {
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 重启应用（重置确认后的收尾；never returns）
pub fn restart(app: &AppHandle) {
    app.restart();
}
