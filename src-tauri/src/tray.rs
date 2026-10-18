//! 系统托盘 —— 常驻快捷入口。
//! 托盘图标保持原生（OS 无自绘口）；右键不再挂 Win32 原生菜单——原生菜单只能
//! 跟随系统主题渲染、无法按应用暗色主题着色，故改为「自绘托盘面板」：
//! 隐藏的无边框 WebviewWindow（前端 /tray 路由，全部 --phx-* token 渲染），
//! 右键托盘时按图标位置贴边弹出，失焦 / Esc 自动隐藏（lib.rs on_window_event）。
//! 左键 = 显示主窗口。主窗口点 X 隐藏到托盘（lib.rs 拦截），面板「退出」为唯一真实出口。

use std::sync::Mutex;

use tauri::{
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalSize, Position, Size,
    WebviewUrl, WebviewWindowBuilder,
};

use crate::profile;

const TRAY_ID: &str = "phx-tray";
/// 自绘托盘面板窗口 label（lib.rs 失焦隐藏按此判定；前端 /tray 路由）
pub const TRAY_WIN: &str = "tray";

/// 面板逻辑宽（前端 .wrap 含 8px 阴影呼吸边距）
const PANEL_W: f64 = 264.0;
/// 内容高度缓存缺失时的兜底（前端挂载后会立即上报真实值）
const PANEL_H_DEFAULT: f64 = 420.0;
/// 面板与托盘图标的间距（逻辑像素，物理 = ×scale）
const PANEL_GAP: f64 = 6.0;

/// 面板锚点：x（面板右缘对齐托盘图标右缘，物理像素）+ edge_y（above 时为面板
/// 应贴的底边，below 时为顶边）+ 方向。点开时缓存，供内容高度变化后重新贴边。
struct Anchor {
    x: f64,
    edge_y: f64,
    above: bool,
}

static ANCHOR: Mutex<Option<Anchor>> = Mutex::new(None);
static CONTENT_H: Mutex<f64> = Mutex::new(PANEL_H_DEFAULT);

/// 建托盘（setup 调用一次）：预载隐藏面板窗 + 原生图标
pub fn init(app: &AppHandle) -> tauri::Result<()> {
    // 隐藏的自绘面板窗：启动即加载 /tray 页面，右键时直接弹出（无首次加载闪白）
    WebviewWindowBuilder::new(app, TRAY_WIN, WebviewUrl::App("tray".into()))
        .title("Phoenix-DRX")
        .inner_size(PANEL_W, PANEL_H_DEFAULT)
        .decorations(false)
        .shadow(false)
        .transparent(true)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .visible(false)
        .build()?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(app.default_window_icon().expect("打包图标缺失").clone())
        .tooltip("Phoenix-DRX")
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| match event {
            // 左键抬起 = 显示主窗口
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } => show_main(tray.app_handle()),
            // 右键抬起 = 弹出 / 收起自绘面板（rect 为托盘图标矩形，Windows 上为物理坐标）
            TrayIconEvent::Click {
                button: MouseButton::Right,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } => {
                let (pos, size) = match (rect.position, rect.size) {
                    (Position::Physical(p), Size::Physical(s)) => (p.cast::<f64>(), s.cast::<f64>()),
                    (Position::Logical(p), Size::Logical(s)) => {
                        (p.to_physical(1.0), s.to_physical(1.0))
                    }
                    _ => return,
                };
                toggle_panel(tray.app_handle(), pos, size)
            }
            _ => {}
        })
        .build(app)?;
    Ok(())
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// 右键托盘：已显示则收起；否则按图标矩形定位（右缘对齐、默认上方弹出，
/// 越出屏幕顶则落到下方），通知前端重载数据后显示
fn toggle_panel(app: &AppHandle, icon_pos: PhysicalPosition<f64>, icon_size: PhysicalSize<f64>) {
    let Some(win) = app.get_webview_window(TRAY_WIN) else {
        return;
    };
    if win.is_visible().unwrap_or(false) {
        let _ = win.hide();
        return;
    }
    let scale = win.scale_factor().unwrap_or(1.0);
    let h = *CONTENT_H.lock().unwrap();
    let pw = PANEL_W * scale;
    let ph = h * scale;
    let gap = PANEL_GAP * scale;

    let x = (icon_pos.x + icon_size.width - pw).max(0.0);
    let bottom_y = icon_pos.y - gap;
    let (edge_y, above) = if bottom_y - ph >= 0.0 {
        (bottom_y, true)
    } else {
        (icon_pos.y + icon_size.height + gap, false)
    };

    *ANCHOR.lock().unwrap() = Some(Anchor { x, edge_y, above });
    place(&win, h);
    // 通知前端重载数据（防陈旧），再显示
    let _ = app.emit("tray:open", ());
    let _ = win.show();
    let _ = win.set_focus();
}

/// 按 ANCHOR + 内容高度摆放面板（贴边弹出 / 高度变化微调共用）
fn place(win: &tauri::WebviewWindow, h: f64) {
    let anchor = ANCHOR.lock().unwrap();
    let Some(a) = anchor.as_ref() else {
        return;
    };
    let scale = win.scale_factor().unwrap_or(1.0);
    let y = if a.above {
        a.edge_y - h * scale
    } else {
        a.edge_y
    };
    let _ = win.set_position(PhysicalPosition::new(a.x as i32, y as i32));
    let _ = win.set_size(LogicalSize::new(PANEL_W, h));
}

// —— 供 lib.rs 薄转发为 IPC 命令 ——

/// 退出应用（面板「退出」按钮；唯一真实出口）
pub fn quit(app: &AppHandle) {
    app.exit(0);
}

/// 显示主窗口并收起面板（面板「显示主窗口」按钮）
pub fn show_main_panel(app: &AppHandle) {
    show_main(app);
    if let Some(win) = app.get_webview_window(TRAY_WIN) {
        let _ = win.hide();
    }
}

/// 打开当前服务器存档目录（资源管理器）；无服务器时不动作
pub fn open_profiles_dir(app: &AppHandle) {
    if let Some(dir) = profile::profiles_dir(app) {
        let _ = std::process::Command::new("explorer").arg(dir).spawn();
    }
}

/// 收起面板（Esc / 各动作完成后的统一出口）
pub fn hide_panel(app: &AppHandle) {
    if let Some(win) = app.get_webview_window(TRAY_WIN) {
        let _ = win.hide();
    }
}

/// 前端上报内容高度（bind:clientHeight，逻辑像素）。缓存之；面板显示中则按
/// 锚点重新贴边（存档列表变长 / 变短的无感微调）
pub fn report_height(app: &AppHandle, h: f64) {
    let h = h.clamp(160.0, 640.0);
    *CONTENT_H.lock().unwrap() = h;
    if let Some(win) = app.get_webview_window(TRAY_WIN) {
        if win.is_visible().unwrap_or(false) {
            place(&win, h);
        }
    }
}
