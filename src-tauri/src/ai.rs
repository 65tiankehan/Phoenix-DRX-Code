//! AI 助手底座 —— OpenAI 兼容统一客户端（方案 docs/plans/AI增强一期方案.md，稿图 30）。
//! 服务商三件套（base_url / model / provider）存 settings.json；API 密钥只存系统凭据库
//! （auth.rs 同款 keyring，永不出 Rust、绝不写入 settings.json——隐私边界铁律）。
//! 请求通路：仅在渲染层显式调用时发请求，请求体由调用方组装（助手页只携带最小上下文）。

use serde::Serialize;
use serde_json::Value;
use std::time::Duration;
use tauri::AppHandle;

use crate::auth::{cred_delete, cred_get, cred_set};
use crate::settings;

const CRED_KEY: &str = "ai-api-key";
const OLLAMA_BASE: &str = "http://127.0.0.1:11434";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(90);

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .expect("reqwest client 构建失败")
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiConfigView {
    enabled: bool,
    provider: String,
    base_url: String,
    model: String,
    page_entries: bool,
    has_key: bool,
    /// 密钥尾 4 位供 UI 辨认（绝不回传全量）
    key_hint: Option<String>,
}

fn view(cfg: &settings::AiSettings, has_key: bool, key_hint: Option<String>) -> AiConfigView {
    AiConfigView {
        enabled: cfg.enabled,
        provider: cfg.provider.clone(),
        base_url: cfg.base_url.clone(),
        model: cfg.model.clone(),
        page_entries: cfg.page_entries,
        has_key,
        key_hint,
    }
}

/// 读取 AI 配置视图（密钥只给「有没有 + 尾 4 位」）
#[tauri::command]
pub async fn ai_get_config(app: AppHandle) -> Result<AiConfigView, String> {
    let s = settings::get(&app);
    let key = cred_get(CRED_KEY)?;
    let hint = key.as_ref().and_then(|k| {
        let chars: Vec<char> = k.chars().collect();
        if chars.len() >= 4 {
            Some(chars[chars.len() - 4..].iter().collect())
        } else {
            None
        }
    });
    Ok(view(&s.ai, key.is_some(), hint))
}

/// 保存 AI 配置（三件套 + 开关；密钥走 ai_set_key 单独通道）
#[tauri::command]
pub async fn ai_set_config(app: AppHandle, config: settings::AiSettings) -> Result<(), String> {
    let mut s = settings::get(&app);
    s.ai = config;
    settings::set(&app, s)
}

/// 保存 API 密钥到系统凭据库
#[tauri::command]
pub async fn ai_set_key(key: String) -> Result<(), String> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err("密钥不能为空".into());
    }
    cred_set(CRED_KEY, trimmed)
}

/// 清除 API 密钥
#[tauri::command]
pub async fn ai_clear_key() -> Result<(), String> {
    cred_delete(CRED_KEY)
}

/// 探测本机 Ollama：在线返回模型名列表，离线返回 None（UI 显示「未检测到」）
#[tauri::command]
pub async fn ai_detect_ollama() -> Result<Option<Vec<String>>, String> {
    let resp = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|e| e.to_string())?
        .get(format!("{OLLAMA_BASE}/api/tags"))
        .send()
        .await;
    match resp {
        Err(_) => Ok(None),
        Ok(r) if !r.status().is_success() => Ok(None),
        Ok(r) => {
            let v: Value = r.json().await.map_err(|e| e.to_string())?;
            let names: Vec<String> = v["models"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|m| m["name"].as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            Ok(Some(names))
        }
    }
}

/// 组装一次 chat/completions 请求并取回 assistant 消息
async fn chat_once(base_url: &str, model: &str, key: Option<&str>, mut body: Value) -> Result<Value, String> {
    let obj = body
        .as_object_mut()
        .ok_or_else(|| "请求体必须是 JSON 对象".to_string())?;
    // model 以调用方传入为准，缺省补配置值
    if !obj.contains_key("model") || obj["model"].as_str().unwrap_or("").is_empty() {
        obj.insert("model".into(), Value::String(model.to_string()));
    }
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let mut req = http().post(&url).json(&body);
    // Ollama 本地服务不需要鉴权头；云端缺密钥直接报错（fail loud，不静默降级）
    if base_url.starts_with(OLLAMA_BASE) {
        // 无 Authorization
    } else {
        let key = key.ok_or_else(|| "未配置 API 密钥（设置页保存后重试）".to_string())?;
        req = req.bearer_auth(key);
    }
    let resp = req.send().await.map_err(|e| format!("请求失败：{e}"))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("服务端返回 {status}：{}", truncate(&text, 300)));
    }
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("响应解析失败：{e}"))?;
    let msg = v["choices"][0]["message"].clone();
    if msg.is_null() {
        return Err(format!("响应缺少 message：{}", truncate(&text, 300)));
    }
    Ok(msg)
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n).collect::<String>() + "…"
    }
}

/// 连接测试：最小请求，返回模型回话片段（稿图 30「测试连接」按钮）
#[tauri::command]
pub async fn ai_test_connection(app: AppHandle) -> Result<String, String> {
    let s = settings::get(&app);
    let key = cred_get(CRED_KEY)?;
    let body = serde_json::json!({
        "messages": [{"role": "user", "content": "回复「连接正常」四个字"}],
        "max_tokens": 16
    });
    let msg = chat_once(&s.ai.base_url, &s.ai.model, key.as_deref(), body).await?;
    Ok(msg["content"]
        .as_str()
        .map(|c| truncate(c, 60))
        .unwrap_or_else(|| "（空回复，但通路正常）".into()))
}

/// 助手对话通路：请求体（messages/tools/tool_choice）由调用方组装透传，
/// 返回 assistant message（含 content 与 tool_calls）。白名单校验在执行层，不在此处。
#[tauri::command]
pub async fn ai_chat(app: AppHandle, body: Value) -> Result<Value, String> {
    let s = settings::get(&app);
    let key = cred_get(CRED_KEY)?;
    chat_once(&s.ai.base_url, &s.ai.model, key.as_deref(), body).await
}
