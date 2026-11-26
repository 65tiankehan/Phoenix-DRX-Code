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

/// AI 助手配置（稿图 30）。密钥绝不入此文件——走系统凭据库（ai.rs CRED_KEY）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AiSettings {
    /// 总开关：关闭后命令面板与页面 AI 入口全部隐藏
    pub enabled: bool,
    /// 服务商标识：deepseek / glm / siliconflow / ollama / custom
    pub provider: String,
    /// OpenAI 兼容 base_url（不含 /chat/completions）
    pub base_url: String,
    /// 模型名
    pub model: String,
    /// 各功能页「AI 推荐」入口开关
    pub page_entries: bool,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: "deepseek".into(),
            base_url: "https://api.deepseek.com/v1".into(),
            model: "deepseek-chat".into(),
            page_entries: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// 自动补全缺失的武器精通/专精等级
    pub auto_add_masterings: bool,
    /// 自动补全缺失的 Scav 技能等级
    pub auto_add_scav_skills: bool,
    /// 问题处理三态：ask / fix / ignore
    pub issue_handling: String,
    /// 启动时静默检查更新（手动检查不受此开关限制）
    pub check_updates: bool,
    /// 首页动态条是否展示赞助推荐（关闭后槽位回退公告/小贴士）
    pub show_sponsor: bool,
    /// 是否显示真实物品图标（关闭后一律用首字色块，也不联网下载）
    pub item_icons: bool,
    /// AI 助手配置（密钥存系统凭据库，不入本文件）
    pub ai: AiSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_add_masterings: false,
            auto_add_scav_skills: false,
            issue_handling: ISSUE_ASK.to_string(),
            check_updates: true,
            show_sponsor: true,
            item_icons: true,
            ai: AiSettings::default(),
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
