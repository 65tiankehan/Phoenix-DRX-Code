//! 应用更新服务 —— tauri-plugin-updater 封装 + 更新薄窗生命周期。
//! 状态机对齐蓝本（tengri-novel updaterIpc.ts）：
//!   idle → checking → available / notAvailable / error
//!   available → downloading → downloaded / error（下载失败回 available 可重试）
//! 与蓝本的两处差异（稿图 17-update 裁决）：
//!   1. 不自动下载：发现新版本后由用户点「立即更新」才开始下载；
//!   2. 「后台下载」= 关窗继续下载（蓝本是取消下载并关窗）。
//! 全部状态经 app.emit("update://status") 广播，主窗与薄窗同听。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_updater::UpdaterExt;

pub const UPDATE_WIN: &str = "update";
pub const EVENT_STATUS: &str = "update://status";

/// 薄窗尺寸（对齐稿图 17-update：420 宽、内容自适应）
const WIN_W: f64 = 420.0;
const WIN_H: f64 = 380.0;

// ----- 状态结构（serde camelCase，前端字段名以此为准） -----

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "phase")]
pub enum UpdateStatus {
    /// 尚未检查
    Idle,
    /// 检查中
    Checking,
    /// 发现新版本（不自动下载，等用户点「立即更新」）
    Available {
        version: String,
        notes: String,
    },
    /// 已是最新
    NotAvailable,
    /// 下载中（received/total 字节）
    Downloading {
        received: u64,
        total: Option<u64>,
    },
    /// 下载完成，待重启安装
    Downloaded {
        version: String,
    },
    /// 出错（启动静默检查失败不广播此态，静默回 idle）
    Error {
        message: String,
    },
}

/// 「稍后提醒」：本次会话不再因启动检查打扰用户（手动检查不受限）
static DISMISSED: AtomicBool = AtomicBool::new(false);

/// 待安装/待下载的更新载荷（check 成功存入，install 取出）
static PENDING: Mutex<Option<tauri_plugin_updater::Update>> = Mutex::new(None);

/// 当前状态快照（update_get_status 读取，薄窗挂载时先取快照再听广播）
static CURRENT: Mutex<Option<UpdateStatus>> = Mutex::new(None);

/// 下载完成的安装包字节，install 时喂给 update.install
static DOWNLOADED: Mutex<Option<Vec<u8>>> = Mutex::new(None);

/// 开薄窗时被收起的其他可见窗口 label（薄窗销毁时按序还原）。
/// 更新薄窗是专职窗口：开窗收起其余可见窗口，关窗还原——对齐蓝本 updateWindow.ts。
static HIDDEN_OTHERS: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn broadcast(app: &AppHandle, status: &UpdateStatus) {
    *CURRENT.lock().unwrap() = Some(status.clone());
    let _ = app.emit(EVENT_STATUS, status);
}

pub fn current_status() -> UpdateStatus {
    CURRENT
        .lock()
        .unwrap()
        .clone()
        .unwrap_or(UpdateStatus::Idle)
}

// ----- 核心动作 -----

/// 检查更新。manual=true（设置页手动检查）时无论是否点过「稍后提醒」都开窗反馈；
/// manual=false（启动静默检查）只在未点过「稍后提醒」时开窗，失败静默回 idle。
/// 返回终态给调用方（设置页据 available/notAvailable/error 做 toast）。
pub async fn check(app: AppHandle, manual: bool) -> Result<UpdateStatus, String> {
    broadcast(&app, &UpdateStatus::Checking);
    let result = app.updater().map_err(|e| e.to_string())?.check().await;
    match result {
        Ok(Some(update)) => {
            let version = update.version.clone();
            let notes = update.body.clone().unwrap_or_default();
            *PENDING.lock().unwrap() = Some(update);
            let status = UpdateStatus::Available { version, notes };
            broadcast(&app, &status);
            if manual || !DISMISSED.load(Ordering::Relaxed) {
                open_window(&app)?;
            }
            Ok(status)
        }
        Ok(None) => {
            broadcast(&app, &UpdateStatus::NotAvailable);
            Ok(UpdateStatus::NotAvailable)
        }
        Err(e) => {
            if manual {
                let status = UpdateStatus::Error {
                    message: e.to_string(),
                };
                broadcast(&app, &status);
                Ok(status)
            } else {
                broadcast(&app, &UpdateStatus::Idle);
                Ok(UpdateStatus::Idle)
            }
        }
    }
}

/// 开始下载。任务在独立运行时里跑，关窗（后台下载）不打断；
/// 成功后进 downloaded，失败回 available 让用户可重试。
pub fn download(app: AppHandle) -> Result<(), String> {
    // 正在下载就别重复起任务
    if matches!(current_status(), UpdateStatus::Downloading { .. }) {
        return Ok(());
    }
    let update = PENDING.lock().unwrap().take();
    let Some(update) = update else {
        return Err("没有待下载的更新".to_string());
    };
    broadcast(&app, &UpdateStatus::Downloading { received: 0, total: None });
    tauri::async_runtime::spawn(async move {
        let progress_app = app.clone();
        let mut received: u64 = 0;
        let result = update
            .download(
                |chunk, total| {
                    received += chunk as u64;
                    broadcast(
                        &progress_app,
                        &UpdateStatus::Downloading {
                            received,
                            total,
                        },
                    );
                },
                || {},
            )
            .await;
        match result {
            Ok(bytes) => {
                let version = update.version.clone();
                *DOWNLOADED.lock().unwrap() = Some(bytes);
                *PENDING.lock().unwrap() = Some(update);
                broadcast(&app, &UpdateStatus::Downloaded { version });
            }
            Err(e) => {
                // 下载失败：载荷放回（重试可再取），状态回 available
                let status = UpdateStatus::Available {
                    version: update.version.clone(),
                    notes: update.body.clone().unwrap_or_default(),
                };
                *PENDING.lock().unwrap() = Some(update);
                broadcast(&app, &status);
                let _ = e;
            }
        }
    });
    Ok(())
}

/// 立即安装：退出应用并运行安装器（windows.installMode = passive）
pub fn install(_app: AppHandle) -> Result<(), String> {
    let bytes = DOWNLOADED.lock().unwrap().take();
    let Some(bytes) = bytes else {
        return Err("没有待安装的更新".to_string());
    };
    let update = PENDING.lock().unwrap().take();
    let Some(update) = update else {
        return Err("没有待安装的更新".to_string());
    };
    update.install(bytes.as_slice()).map_err(|e| e.to_string())
}

/// 开薄窗前收起其余可见窗口（已收起过则跳过，薄窗复用路径不重复记录）
fn hide_others(app: &AppHandle) {
    let mut hidden = HIDDEN_OTHERS.lock().unwrap();
    if !hidden.is_empty() {
        return;
    }
    for (label, win) in app.webview_windows() {
        if label == UPDATE_WIN {
            continue;
        }
        if win.is_visible().unwrap_or(false) && win.hide().is_ok() {
            hidden.push(label);
        }
    }
}

/// 薄窗销毁时还原开窗前收起的窗口（「稍后提醒」「后台下载」等一切关窗路径统一走这；
/// 由 lib.rs 全局窗口事件在 Destroyed 且 label=update 时调用）
pub fn restore_others(app: &AppHandle) {
    let mut hidden = HIDDEN_OTHERS.lock().unwrap();
    for label in hidden.drain(..) {
        if let Some(win) = app.get_webview_window(&label) {
            let _ = win.show();
        }
    }
}

/// 打开更新薄窗（单例：已存在则 show + focus）。
/// 专职窗口语义：开窗收起其余可见窗口，薄窗销毁时还原（蓝本 updateWindow.ts）。
pub fn open_window(app: &AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(UPDATE_WIN) {
        hide_others(app);
        let _ = win.show();
        let _ = win.set_focus();
        return Ok(());
    }
    hide_others(app);
    WebviewWindowBuilder::new(app, UPDATE_WIN, WebviewUrl::App("update-window".into()))
        .title("Phoenix-DRX")
        .inner_size(WIN_W, WIN_H)
        .center()
        .decorations(false)
        .shadow(false)
        .transparent(true)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 「稍后提醒」：本会话不再因启动检查打扰，并真关薄窗
pub fn dismiss(app: AppHandle) -> Result<(), String> {
    DISMISSED.store(true, Ordering::Relaxed);
    if let Some(win) = app.get_webview_window(UPDATE_WIN) {
        let _ = win.close();
    }
    Ok(())
}
