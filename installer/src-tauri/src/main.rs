#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! DRX-Setup：Phoenix-DRX 自研安装器（方案 docs/plans/自研安装器方案.md，稿图 28）。
//! 复用模板设计：应用差异全部在 installer-config.json，代码不改。
//! 载荷：CI 用 scripts/append-payload.mjs 把主程序 zip 拼到本 exe 尾部（trailer 定位），
//! 运行时读自身解压落盘；per-user 安装（%LOCALAPPDATA%\Programs），写 HKCU 卸载登记。

use serde::Serialize;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

/// 配置单一来源（复用模板：新项目只改 installer-config.json）
const CONFIG_JSON: &str = include_str!("../../installer-config.json");

/// 载荷 trailer：[zip][u64 zip_len LE][u64 zip_offset LE][magic 8B] 共 24 字节（append-payload.mjs 对应）
const TRAILER_MAGIC: &[u8; 8] = b"DRXSETUP";
const TRAILER_SIZE: u64 = 24;

const EVENT_PROGRESS: &str = "install://progress";

/// 安装中止旗标（拍板 ②：解压开始后中止需确认，由前端弹确认后调 install_cancel）
static CANCELLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// WebView2 运行时检测（微软官方口径：两处 pv 注册表键，非空且非 0.0.0.0 即已装；
/// 方案 §2.3/§四.5——注册表缺失的极端个案接受漏检兜底：Tauri 壳起不来时用户会看到本机消息框引导）
#[cfg(windows)]
fn webview2_installed() -> bool {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};
    use winreg::RegKey;
    const HKLM_KEY: &str = r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
    const HKCU_KEY: &str = r"Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
    let pv_ok = |path: &str, predef: windows_sys::Win32::System::Registry::HKEY| -> bool {
        RegKey::predef(predef)
            .open_subkey_with_flags(path, KEY_READ)
            .ok()
            .and_then(|k| k.get_value::<String, _>("pv").ok())
            .map(|v| !v.is_empty() && v != "0.0.0.0")
            .unwrap_or(false)
    };
    pv_ok(HKLM_KEY, HKEY_LOCAL_MACHINE) || pv_ok(HKCU_KEY, HKEY_CURRENT_USER)
}

/// 无 GUI 兜底（WebView2 缺失时 Tauri 壳无法启动）：本机消息框引导 + 打开官方下载页
#[cfg(windows)]
fn webview2_guard() {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONWARNING, MB_OK, MB_SETFOREGROUND};
    fn wide(s: &str) -> Vec<u16> {
        std::ffi::OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
    }
    if !webview2_installed() {
        let (body, title) = if ui_language() == "zh-CN" {
            (
                "Phoenix-DRX 安装器需要系统的 WebView2 运行环境。\n点击「确定」打开微软官方下载页，安装后重新运行本安装器。",
                "Phoenix-DRX 安装",
            )
        } else {
            (
                "The Phoenix-DRX installer needs the Microsoft WebView2 runtime.\nClick OK to open the official download page, then run this installer again.",
                "Phoenix-DRX Setup",
            )
        };
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                wide(body).as_ptr(),
                wide(title).as_ptr(),
                MB_OK | MB_ICONWARNING | MB_SETFOREGROUND,
            );
            ShellExecuteW(
                std::ptr::null_mut(),
                wide("open").as_ptr(),
                wide("https://go.microsoft.com/fwlink/p/?LinkId=2124703").as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1, // SW_SHOWNORMAL
            );
        }
        std::process::exit(2);
    }
}

#[cfg(not(windows))]
fn webview2_guard() {}

#[derive(serde::Deserialize, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Config {
    app_id: String,
    display_name: String,
    exe_name: String,
    publisher: String,
    version: String,
    tagline: String,
    /// 英文副标题；缺省时回退 `tagline`（老配置兼容）
    #[serde(default)]
    tagline_en: Option<String>,
    default_dir_name: String,
    payload_size_mb: f64,
    shortcuts: ShortcutsCfg,
    #[serde(default)]
    uninstall: UninstallCfg,
    ads: AdsCfg,
}

#[derive(serde::Deserialize, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ShortcutsCfg {
    desktop: bool,
    start_menu: bool,
}

#[derive(serde::Deserialize, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdsCfg {
    enabled: bool,
    slides: Vec<AdsSlide>,
}

#[derive(serde::Deserialize, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct UninstallCfg {
    keep_user_data: bool,
    data_dirs: Vec<String>,
}

#[derive(serde::Deserialize, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdsSlide {
    kind: String,
    color: String,
    title: String,
    desc: String,
    /// 英文版文案；缺省时前端回退中文字段
    #[serde(default)]
    title_en: Option<String>,
    #[serde(default)]
    desc_en: Option<String>,
    url: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LaunchInfo {
    config: Config,
    default_dir: String,
    install_dir: String,
    launch_mode: String, // install | uninstall
    /// 安装器界面语言（`zh-CN` | `en-US`），由系统显示语言决定，与 WebView 的 navigator.language 保持一致
    ui_language: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    phase: String, // extracting | registry | shortcuts | done | error
    percent: f64,
    step: String,
}

struct InstallState {
    running: Mutex<bool>,
}

fn config() -> Config {
    serde_json::from_str(CONFIG_JSON).expect("installer-config.json 非法")
}

/// 安装器界面语言：取系统显示语言的主语言 ID（低 10 位），中文家族 → `zh-CN`，其余一律 `en-US`。
/// Rust 侧消息框与 WebView 页面共用这一个判断，避免两边语言不一致。
/// 调试入口：环境变量 `PHX_INSTALLER_LANG=en-US|zh-CN` 可强制语言，用于在中文机上核对英文界面（不改变正式行为）。
#[cfg(windows)]
fn ui_language() -> &'static str {
    if let Ok(forced) = std::env::var("PHX_INSTALLER_LANG") {
        if forced.eq_ignore_ascii_case("en-US") {
            return "en-US";
        }
        if forced.eq_ignore_ascii_case("zh-CN") {
            return "zh-CN";
        }
    }
    // SAFETY: GetUserDefaultUILanguage 无参数、无副作用；失败时系统返回默认语言 ID，不会 panic。
    let langid = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
    if langid & 0x3FF == 0x04 {
        "zh-CN"
    } else {
        "en-US"
    }
}

#[cfg(not(windows))]
fn ui_language() -> &'static str {
    "en-US"
}

fn default_install_dir() -> PathBuf {
    let local = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| r"C:\Program Files".into());
    Path::new(&local).join("Programs").join(&config().default_dir_name)
}

/// 升级迁移探测：机器上已有安装就用它的目录（覆盖升级），否则用我们的默认目录。
/// 优先 NSIS 旧登记项（Tauri NSIS per-user 默认 `%LOCALAPPDATA%\{productName}`，无 Programs 子层，
/// 与我们默认目录不同——v0.1.8 首次真机更新曾因此装出双份，快捷方式仍指旧家），其次自身登记项。
#[cfg(windows)]
fn detect_existing_install_dir(cfg: &Config) -> Option<PathBuf> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
    use winreg::RegKey;
    let root = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Uninstall")
        .ok()?;
    for name in [cfg.display_name.as_str(), cfg.app_id.as_str()] {
        if let Ok(k) = root.open_subkey_with_flags(name, KEY_READ) {
            if let Ok(loc) = k.get_value::<String, _>("InstallLocation") {
                // Tauri NSIS 写入的值自带一对引号（真机取证 2026-10-24：NSIS 键值打印带引号、
                // 自研键不带），不清洗则 PathBuf 含引号、is_dir 恒 false，迁移探测必失明
                let cleaned = loc.trim().trim_matches('"');
                let p = PathBuf::from(cleaned);
                if p.is_dir() && p.join(&cfg.exe_name).is_file() {
                    return Some(p);
                }
            }
        }
    }
    None
}

#[cfg(not(windows))]
fn detect_existing_install_dir(_cfg: &Config) -> Option<PathBuf> {
    None
}

fn resolve_default_dir() -> PathBuf {
    detect_existing_install_dir(&config()).unwrap_or_else(default_install_dir)
}

/// 读自身 exe 尾部 trailer，取出载荷 zip 字节（开发模式未拼接时报错提示）
fn read_payload() -> Result<Vec<u8>, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut f = fs::File::open(&exe).map_err(|e| e.to_string())?;
    let len = f.metadata().map_err(|e| e.to_string())?.len();
    if len < TRAILER_SIZE {
        return Err("安装包缺少载荷：请先用 scripts/append-payload.mjs 拼接再运行".into());
    }
    f.seek(SeekFrom::Start(len - TRAILER_SIZE)).map_err(|e| e.to_string())?;
    let mut tail = [0u8; 24];
    f.read_exact(&mut tail).map_err(|e| e.to_string())?;
    if &tail[16..24] != TRAILER_MAGIC {
        return Err("安装包缺少载荷：请先用 scripts/append-payload.mjs 拼接再运行".into());
    }
    let zip_len = u64::from_le_bytes(tail[0..8].try_into().unwrap());
    let offset = len - TRAILER_SIZE - zip_len;
    f.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
    let mut zip_bytes = vec![0u8; zip_len as usize];
    f.read_exact(&mut zip_bytes).map_err(|e| e.to_string())?;
    Ok(zip_bytes)
}

fn emit(app: &AppHandle, phase: &str, percent: f64, step: &str) {
    let _ = app.emit(EVENT_PROGRESS, Progress { phase: phase.into(), percent, step: step.into() });
}

/// 解压载荷 zip 到目标目录（enclosed_name 防路径穿越；进度按条目数广播）
fn extract_payload(zip_bytes: &[u8], target: &Path, app: &AppHandle) -> Result<(), String> {
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(zip_bytes)).map_err(|e| e.to_string())?;
    let total = archive.len().max(1) as f64;
    for i in 0..archive.len() {
        if CANCELLED.load(std::sync::atomic::Ordering::Relaxed) {
            return Err("已中止安装".into());
        }
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(name) = entry.enclosed_name() else { continue };
        let out = target.join(name);
        if entry.is_dir() {
            fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(p) = out.parent() {
            fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let mut w = fs::File::create(&out).map_err(|e| format!("文件被占用或无权限（{}）：{}", out.display(), e))?;
        std::io::copy(&mut entry, &mut w).map_err(|e| e.to_string())?;
        emit(app, "extracting", (i as f64 + 1.0) / total * 90.0, "解压程序文件");
    }
    Ok(())
}

fn dir_size_kb(target: &Path) -> u64 {
    fn walk(dir: &Path) -> u64 {
        let mut n = 0;
        if let Ok(rd) = fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    n += walk(&p);
                } else if let Ok(m) = e.metadata() {
                    n += m.len();
                }
            }
        }
        n
    }
    walk(target) / 1024
}

/// HKCU 卸载登记（exe 安装器标准做法；S3 卸载器删除此键）
fn write_uninstall_registry(cfg: &Config, target: &Path) -> Result<(), String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key_path = format!(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{}", cfg.app_id);
    let (key, _) = hkcu.create_subkey(&key_path).map_err(|e| e.to_string())?;
    let uninstaller = target.join("Uninstall.exe");
    key.set_value("DisplayName", &cfg.display_name).map_err(|e| e.to_string())?;
    key.set_value("DisplayVersion", &cfg.version).map_err(|e| e.to_string())?;
    key.set_value("Publisher", &cfg.publisher).map_err(|e| e.to_string())?;
    key.set_value("InstallLocation", &target.to_string_lossy().to_string()).map_err(|e| e.to_string())?;
    key.set_value("DisplayIcon", &target.join(&cfg.exe_name).to_string_lossy().to_string()).map_err(|e| e.to_string())?;
    key.set_value("UninstallString", &format!("\"{}\"", uninstaller.display())).map_err(|e| e.to_string())?;
    key.set_value("NoModify", &1u32).map_err(|e| e.to_string())?;
    key.set_value("NoRepair", &1u32).map_err(|e| e.to_string())?;
    key.set_value("EstimatedSize", &(dir_size_kb(target) as u32)).map_err(|e| e.to_string())?;
    Ok(())
}

/// 真实桌面目录：读 Explorer 的 User Shell Folders（OneDrive 重定向时 USERPROFILE\Desktop 是错的）。
/// REG_EXPAND_SZ 的 %USERPROFILE% 手动展开；读不到回退传统路径。
fn desktop_dir() -> PathBuf {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
    use winreg::RegKey;
    let fallback = || {
        let profile = std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\Users\\Public".into());
        Path::new(&profile).join("Desktop")
    };
    let Some(key) = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(r"Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders", KEY_READ)
        .ok()
    else {
        return fallback();
    };
    let Some(raw) = key.get_value::<String, _>("Desktop").ok() else {
        return fallback();
    };
    let profile = std::env::var("USERPROFILE").unwrap_or_default();
    PathBuf::from(raw.replace("%USERPROFILE%", &profile))
}

/// 快捷方式（mslnk 纯 Rust 写 .lnk）
fn create_shortcuts(cfg: &Config, target: &Path, desktop: bool, start_menu: bool) -> Result<(), String> {
    let exe = target.join(&cfg.exe_name);
    let make = |link: &Path| -> Result<(), String> {
        mslnk::ShellLink::new(&exe).map_err(|e| e.to_string())?.create_lnk(link).map_err(|e| e.to_string())
    };
    if start_menu {
        let appdata = std::env::var("APPDATA").map_err(|_| "缺少 APPDATA 环境变量".to_string())?;
        let dir = Path::new(&appdata).join(r"Microsoft\Windows\Start Menu\Programs");
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        make(&dir.join(format!("{}.lnk", cfg.display_name)))?;
    }
    if desktop {
        let dir = desktop_dir();
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        make(&dir.join(format!("{}.lnk", cfg.display_name)))?;
    }
    Ok(())
}

/// 全流程：结束主程序 → 解压 90% → 登记 94% → 快捷方式 98% → done 100%。
/// 返回是否成功（静默更新链据此拉起新版）。
fn run_install(app: AppHandle, target: PathBuf, desktop_sc: bool, start_menu_sc: bool) -> bool {
    let cfg = config();
    // 升级迁移：写新登记前先取旧自研登记的位置（v0.1.8 可能装在 Programs 子层的旧家）
    let prev_own_dir: Option<PathBuf> = {
        use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
        use winreg::RegKey;
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(
                format!(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{}", cfg.app_id),
                KEY_READ,
            )
            .ok()
            .and_then(|k| k.get_value::<String, _>("InstallLocation").ok())
            .map(PathBuf::from)
    };
    let ok = match (|| -> Result<(), String> {
        // 应用内更新链的主程序此刻还在运行（NSIS 由安装脚本杀进程，我们自己做）：
        // 静默安装开始前先结束主程序，否则解压必撞「文件被占用」
        let _ = cmd_hidden(&["/C", "taskkill", "/F", "/IM", &cfg.exe_name]);
        std::thread::sleep(std::time::Duration::from_millis(300));
        fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        let payload = read_payload()?;
        extract_payload(&payload, &target, &app)?;
        // 卸载器 = 自身副本（带 --uninstall 参数启动进卸载模式）
        let self_exe = std::env::current_exe().map_err(|e| e.to_string())?;
        fs::copy(&self_exe, target.join("Uninstall.exe")).map_err(|e| e.to_string())?;
        emit(&app, "registry", 94.0, "写入安装登记（添加/删除程序）");
        write_uninstall_registry(&cfg, &target)?;
        emit(&app, "shortcuts", 98.0, "创建快捷方式");
        create_shortcuts(&cfg, &target, desktop_sc, start_menu_sc)?;
        Ok(())
    })() {
        Ok(()) => {
            // 迁移收编：清掉旧自研残留目录（与本次目标不同且确属我们的安装：主程序+卸载器俱全才动），
            // 删 NSIS 旧登记键——自此「添加/删除程序」只剩我们一条
            if let Some(prev) = prev_own_dir {
                if prev != target && prev.join(&cfg.exe_name).is_file() && prev.join("Uninstall.exe").is_file() {
                    rm_rf(&prev);
                }
            }
            {
                use winreg::enums::HKEY_CURRENT_USER;
                use winreg::RegKey;
                let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey(format!(
                    r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{}",
                    cfg.display_name
                ));
            }
            // 拆态自愈（治 0.1.9 引号失明事故造成的双目录）：NSIS 老家目录若仍存在且含主程序，
            // 视为残留——删除并把桌面/开始菜单快捷方式重指到本次目标（老快捷方式指旧目录会开到旧版）
            if let Some(local) = std::env::var("LOCALAPPDATA").ok() {
                let legacy = Path::new(&local).join(&cfg.display_name);
                if legacy != target && legacy.join(&cfg.exe_name).is_file() {
                    rm_rf(&legacy);
                    let _ = create_shortcuts(&cfg, &target, true, true);
                }
            }
            emit(&app, "done", 100.0, "安装完成");
            true
        }
        Err(e) => {
            emit(&app, "error", 0.0, &e);
            false
        }
    };
    *app.state::<InstallState>().running.lock().unwrap() = false;
    ok
}

#[tauri::command]
fn installer_info() -> LaunchInfo {
    let mode = if std::env::args().any(|a| a == "--uninstall") { "uninstall" } else { "install" };
    let install_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    LaunchInfo {
        config: config(),
        default_dir: resolve_default_dir().to_string_lossy().to_string(),
        install_dir,
        launch_mode: mode.into(),
        ui_language: ui_language().into(),
    }
}

#[tauri::command]
async fn pick_dir(app: AppHandle) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .blocking_pick_folder()
            .and_then(|p| p.into_path().ok())
            .map(|p| p.to_string_lossy().to_string())
    })
    .await
    .unwrap_or(None)
}

#[tauri::command]
fn install_start(
    app: AppHandle,
    state: tauri::State<InstallState>,
    target_dir: String,
    desktop: bool,
    start_menu: bool,
) -> Result<(), String> {
    let mut running = state.running.lock().unwrap();
    if *running {
        return Err("安装正在进行".into());
    }
    *running = true;
    drop(running);
    CANCELLED.store(false, std::sync::atomic::Ordering::Relaxed);
    tauri::async_runtime::spawn(async move {
        run_install(app, PathBuf::from(target_dir), desktop, start_menu);
    });
    Ok(())
}

/// 完成「立即运行」：直接拉起主程序（S3 换 ShellExecute 以支持以管理员权限等方式启动）
#[tauri::command]
fn launch_app(target_dir: String) -> Result<(), String> {
    let cfg = config();
    let exe = PathBuf::from(&target_dir).join(&cfg.exe_name);
    std::process::Command::new(&exe)
        .spawn()
        .map_err(|e| format!("启动主程序失败：{}", e))?;
    Ok(())
}

/// 中止安装（前端确认后调用；解压循环每条目检查旗标）
#[tauri::command]
fn install_cancel() {
    CANCELLED.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Windows 下静默执行 cmd，不闪黑框
#[cfg(windows)]
fn cmd_hidden(args: &[&str]) -> std::io::Result<std::process::Output> {
    use std::os::windows::process::CommandExt;
    std::process::Command::new("cmd")
        .args(args)
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .output()
}

fn rm_rf(path: &Path) {
    if path.is_dir() {
        if let Ok(rd) = fs::read_dir(path) {
            for e in rd.flatten() {
                rm_rf(&e.path());
            }
        }
        let _ = fs::remove_dir(path);
    } else {
        let _ = fs::remove_file(path);
    }
}

/// 卸载全流程（方案 §2.4）：结束主程序 → 清快捷方式 → 清登记 → 清数据（可选）→
/// 删目录内容（跳过运行中的 Uninstall.exe 自身）→ 延迟 cmd 自删除。
#[cfg(windows)]
fn run_uninstall(app: AppHandle, keep_data: bool) {
    let cfg = config();
    let Ok(self_exe) = std::env::current_exe() else {
        emit(&app, "error", 0.0, "无法定位卸载器自身");
        return;
    };
    let Some(dir) = self_exe.parent().map(Path::to_path_buf) else {
        emit(&app, "error", 0.0, "无法定位安装目录");
        return;
    };
    // 护栏：目录里没有主程序 exe 就拒绝卸载（防呆：误双击/拿错目录不至于乱删）
    if !dir.join(&cfg.exe_name).exists() {
        emit(&app, "error", 0.0, &format!("未在此目录发现 {}，疑似非安装目录，已拒绝卸载", cfg.exe_name));
        *app.state::<InstallState>().running.lock().unwrap() = false;
        return;
    }
    emit(&app, "killing", 10.0, "结束正在运行的主程序");
    let _ = cmd_hidden(&["/C", "taskkill", "/F", "/IM", &cfg.exe_name]);
    std::thread::sleep(std::time::Duration::from_millis(300));

    emit(&app, "removing", 40.0, "清理快捷方式与安装登记");
    if let Ok(appdata) = std::env::var("APPDATA") {
        let _ = fs::remove_file(
            Path::new(&appdata)
                .join(r"Microsoft\Windows\Start Menu\Programs")
                .join(format!("{}.lnk", cfg.display_name)),
        );
    }
    let _ = fs::remove_file(desktop_dir().join(format!("{}.lnk", cfg.display_name)));
    {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;
        let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey(format!(
            r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{}",
            cfg.app_id
        ));
    }
    if !keep_data {
        emit(&app, "removing", 60.0, "删除个人数据");
        let appdata = std::env::var("APPDATA").unwrap_or_default();
        for tpl in &cfg.uninstall.data_dirs {
            let expanded = tpl.replace("%APPDATA%", &appdata).replace('/', "\\");
            rm_rf(Path::new(&expanded));
        }
    }

    emit(&app, "removing", 80.0, "删除程序文件");
    if let Ok(rd) = fs::read_dir(&dir) {
        for e in rd.flatten() {
            if e.path() != self_exe {
                rm_rf(&e.path());
            }
        }
    }
    // 自删除：卸载器 exe 运行中删不掉自己，退出前挂延迟 cmd 清掉整个目录（方案 §四.1）
    let dir_str = dir.to_string_lossy().to_string();
    let _ = cmd_hidden(&["/C", "timeout /t 2 /nobreak >nul & rmdir /s /q", &format!("\"{}\"", dir_str)]);
    emit(&app, "done", 100.0, "卸载完成");
    *app.state::<InstallState>().running.lock().unwrap() = false;
    std::thread::sleep(std::time::Duration::from_millis(400)); // 给前端一帧看到完成
    app.exit(0);
}

#[cfg(not(windows))]
fn run_uninstall(app: AppHandle, _keep_data: bool) {
    let _ = app;
}

#[tauri::command]
fn uninstall_start(
    app: AppHandle,
    state: tauri::State<InstallState>,
    keep_data: bool,
) -> Result<(), String> {
    let mut running = state.running.lock().unwrap();
    if *running {
        return Err("卸载正在进行".into());
    }
    *running = true;
    drop(running);
    tauri::async_runtime::spawn(async move {
        run_uninstall(app, keep_data);
    });
    Ok(())
}

fn main() {
    webview2_guard();
    // 静默判定：/S（NSIS 静默）与 /P（tauri-plugin-updater passive 模式所用标志）均视为无 UI 全自动安装
    let silent = std::env::args()
        .any(|a| matches!(a.to_ascii_lowercase().as_str(), "/s" | "/p"));
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(InstallState { running: Mutex::new(false) })
        .invoke_handler(tauri::generate_handler![
            installer_info, pick_dir, install_start, install_cancel, launch_app, uninstall_start
        ])
        .setup(move |app| {
            apply_window_rounding(app.handle());
            if silent {
                // 静默模式（/S /P，服务应用内更新链）：无 UI，迁移探测目录，装完拉起新版再退
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.hide();
                }
                let handle = app.handle().clone();
                let dir = resolve_default_dir();
                // debug 构建有控制台，release 静默无窗——排查静默装错目录全靠这行日志
                eprintln!("[DRX-Setup] 静默目标目录 = {}", dir.display());
                let sc = config().shortcuts;
                tauri::async_runtime::spawn(async move {
                    let ok = run_install(handle, dir.clone(), sc.desktop, sc.start_menu);
                    // 「重启后生效」闭环：装成功才拉起新版（失败留给用户手动装，别拉起旧版装糊涂）
                    if ok {
                        let exe = dir.join(config().exe_name);
                        let _ = std::process::Command::new(exe).spawn();
                    }
                    std::process::exit(0);
                });
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("DRX-Setup 启动失败");
}

/// 无边框透明窗请求 DWM 圆角（Win11 一致；对齐主程序 lib.rs 同名函数，消除四角方形残留）
#[cfg(windows)]
fn apply_window_rounding(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(w) = app.get_webview_window("main") {
        if let Ok(hwnd) = w.hwnd() {
            use windows_sys::Win32::Graphics::Dwm::{
                DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
            };
            let pref = DWMWCP_ROUND as u32;
            unsafe {
                DwmSetWindowAttribute(
                    hwnd.0,
                    DWMWA_WINDOW_CORNER_PREFERENCE as u32,
                    &pref as *const u32 as *const core::ffi::c_void,
                    4,
                );
            }
        }
    }
}

#[cfg(not(windows))]
fn apply_window_rounding(_app: &tauri::AppHandle) {}
