//! Phoenix-DRX Rust 后端入口
//!
//! 数据层（服务器发现 / 备份 / 档案读写）对齐旧 Electron IPC 语义：
//! server:pick/add/list/remove/use/current、profile:list/load/save/current/use
//! （IPC 通道冒号改为命令名下划线，前端由 src/lib/ipc.ts 屏蔽差异）。
//! 任何写档案命令执行前必须先落备份（领域铁律：备份先行）。

mod announcement;
mod ai;
mod clothing;
mod arsenal;
mod assistant;
mod auth;
mod backup;
mod character;
mod encyclopedia;
mod health;
mod hideout;
mod inspect;
mod issues;
mod presets;
mod profile;
mod quests;
mod scav;
mod server;
mod settings;
mod skills;
mod stash;
mod sync;
mod traders;
mod tray;
mod updater;

use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

use announcement::AnnouncementPayload;
use auth::{auth_device_cancel, auth_device_start, auth_is_configured, auth_logout};
use backup::BackupRow;
use character::{CharData, CharOptions, CharSaveData};
use health::{HealthData, HealthSaveData};
use hideout::{HideoutData, HideoutSaveData};
use issues::{IssueRow, IssuesFixResult};
use presets::{ImportResult, PresetsData};
use profile::{ProfileHero, ProfileMeta, SaveResult};
use quests::{QuestsData, QuestsSaveData};
use scav::{ScavData, ScavSaveData};
use server::{AddResult, ServerRow};
use settings::Settings;
use skills::{SkillsData, SkillsSaveData};
use stash::{StashData, StashSaveData};
use sync::{sync_pull, sync_push};
use traders::{TradersData, TradersSaveData};

/// 选择 SPT 服务器根目录（系统文件夹选择框）；取消返回 None
#[tauri::command]
async fn server_pick(app: AppHandle) -> Result<Option<String>, String> {
    let picked = app.dialog().file().blocking_pick_folder();
    match picked {
        Some(file) => file
            .into_path()
            .map(|p| Some(p.to_string_lossy().to_string()))
            .map_err(|e| e.to_string()),
        None => Ok(None),
    }
}

/// 登记服务器根目录。无效路径不拒绝，只登记校验状态；重复路径不重复入库
#[tauri::command]
async fn server_add(app: AppHandle, path: String) -> Result<AddResult, String> {
    server::add_server(&app, &path)
}

/// 已登记服务器列表（含实时校验状态）
#[tauri::command]
async fn server_list(app: AppHandle) -> Result<Vec<ServerRow>, String> {
    Ok(server::list_servers(&app))
}

/// 移除登记（幂等）；移除当前选中项时同步清空当前选择
#[tauri::command]
async fn server_remove(app: AppHandle, id: String) -> Result<(), String> {
    server::remove_server(&app, &id)
}

/// 设为当前服务器；返回是否确有此登记
#[tauri::command]
async fn server_use(app: AppHandle, id: String) -> Result<bool, String> {
    server::use_server(&app, &id)
}

/// 当前选中服务器（含实时状态）；未选择或登记已失效时为 None
#[tauri::command]
async fn server_current(app: AppHandle) -> Result<Option<ServerRow>, String> {
    Ok(server::current_server(&app))
}

/// 当前服务器存档列表（解析失败回退 id 作名，仍可入选）
#[tauri::command]
async fn profile_list(app: AppHandle) -> Result<Vec<ProfileMeta>, String> {
    Ok(profile::list_profiles(&app))
}

/// 存档首页数据（角色概览 + 三币种金额）；缺失或不可读时为 None
#[tauri::command]
async fn profile_load(app: AppHandle, id: String) -> Result<Option<ProfileHero>, String> {
    Ok(profile::load_hero(&app, &id))
}

/// 保存写回：先强制备份（领域铁律），再原字节写回
#[tauri::command]
async fn profile_save(app: AppHandle, id: String) -> Result<SaveResult, String> {
    profile::save_profile(&app, &id)
}

/// 当前服务器选中的存档 id；未选择时为 None
#[tauri::command]
async fn profile_current(app: AppHandle) -> Result<Option<String>, String> {
    Ok(profile::get_current_profile_id(&app))
}

/// 设置当前服务器的选中存档
#[tauri::command]
async fn profile_use(app: AppHandle, id: String) -> Result<(), String> {
    profile::set_current_profile(&app, &id)
}

// —— 角色页（03-character）——

/// 角色页全量数据（PMC/Scav 双侧）；档案缺失或不可读时为 None
#[tauri::command]
async fn char_load(app: AppHandle, id: String) -> Result<Option<CharData>, String> {
    Ok(character::load_char(&app, &id))
}

/// 角色页选项：经验表 / 语音 / 头型 / 口袋（来自服务器数据目录）
#[tauri::command]
async fn char_options(app: AppHandle) -> Result<CharOptions, String> {
    Ok(character::load_options(&app))
}

/// 角色页写回：先备份，后定点替换（数值强转整数；阵营仅 PMC 写 Info.Side）
#[tauri::command]
async fn char_save(app: AppHandle, id: String, data: CharSaveData) -> Result<SaveResult, String> {
    character::save_char(&app, &id, &data)
}

// —— 商人页（04-traders）——

/// 商人页全量数据（PMC 商人四字段 + 等级上限 + 跳蚤评分）；档案缺失或不可读时为 None
#[tauri::command]
async fn traders_load(app: AppHandle, id: String) -> Result<Option<TradersData>, String> {
    Ok(traders::load_traders(&app, &id))
}

/// 商人页写回：先备份，只写档案已有商人 token；Fence 同步 scav 好感
#[tauri::command]
async fn traders_save(
    app: AppHandle,
    id: String,
    data: TradersSaveData,
) -> Result<SaveResult, String> {
    traders::save_traders(&app, &id, &data)
}

// —— 任务页（05-quests）——

/// 任务页全量数据（档案任务逐行判型 + 缺失清单）；档案缺失或不可读时为 None
#[tauri::command]
async fn quests_load(app: AppHandle, id: String) -> Result<Option<QuestsData>, String> {
    Ok(quests::load_quests(&app, &id))
}

/// 任务页写回：先备份，差分替换（改状态 / 移除 / 补入 + 联动配方增量）
#[tauri::command]
async fn quests_save(
    app: AppHandle,
    id: String,
    data: QuestsSaveData,
) -> Result<SaveResult, String> {
    quests::save_quests(&app, &id, &data)
}

// —— 技能页（06-skills）——

/// 技能页全量数据（PMC/Scav 双侧 Common/Mastering，每行含上限与本地化名）；档案缺失或不可读时为 None
#[tauri::command]
async fn skills_load(app: AppHandle, id: String) -> Result<Option<SkillsData>, String> {
    Ok(skills::load_skills(&app, &id))
}

/// 技能页写回：先备份，按 side/group/Id 定点替换 Progress（查无跳过不新建）
#[tauri::command]
async fn skills_save(
    app: AppHandle,
    id: String,
    data: SkillsSaveData,
) -> Result<SaveResult, String> {
    skills::save_skills(&app, &id, &data)
}

// —— 健康页（07-health）——

/// 健康页全量数据（PMC/Scav 双侧九项指标）；档案缺失或不可读时为 None
#[tauri::command]
async fn health_load(app: AppHandle, id: String) -> Result<Option<HealthData>, String> {
    Ok(health::load_health(&app, &id))
}

/// 健康页写回：先备份，双侧定点替换（当前 > 上限时上限抬高，写入侧兜底）
#[tauri::command]
async fn health_save(
    app: AppHandle,
    id: String,
    data: HealthSaveData,
) -> Result<SaveResult, String> {
    health::save_health(&app, &id, &data)
}

// —— 藏身处页（08-hideout，仅 PMC）——

/// 藏身处页全量数据（区域 / 配方 / 进行中生产）；档案缺失或不可读时为 None
#[tauri::command]
async fn hideout_load(app: AppHandle, id: String) -> Result<Option<HideoutData>, String> {
    Ok(hideout::load_hideout(&app, &id))
}

/// 藏身处页写回：先备份，整体替换 Areas / Production / unlockedProductionRecipe
/// （区域父子联动与等级卡控在写入侧兜底）
#[tauri::command]
async fn hideout_save(
    app: AppHandle,
    id: String,
    data: HideoutSaveData,
) -> Result<SaveResult, String> {
    hideout::save_hideout(&app, &id, &data)
}

// —— 仓库页（09-stash，仅 PMC）——

/// 仓库页全量数据（仓库表 / 货币 / 附加行数 / 装备槽 / 口袋 / 分类与可添加物品）
#[tauri::command]
async fn stash_load(app: AppHandle, id: String) -> Result<Option<StashData>, String> {
    Ok(stash::load_stash(&app, &id))
}

/// 仓库页写回：先备份，op 列表逐条执行（加钱 / 附加行数 / 删除 / 添加物品）
#[tauri::command]
async fn stash_save(app: AppHandle, id: String, data: StashSaveData) -> Result<SaveResult, String> {
    stash::save_stash(&app, &id, &data)
}

// —— Scav 装备 / 口袋页（11-scav，与仓库同构，数据源换 characters.scav）——

/// Scav 页全量数据（装备槽 / 口袋 / 口袋容器名）
#[tauri::command]
async fn scav_load(app: AppHandle, id: String) -> Result<Option<ScavData>, String> {
    Ok(scav::load_scav(&app, &id))
}

/// Scav 页写回：先备份，op 列表逐条执行（仅删除）
#[tauri::command]
async fn scav_save(app: AppHandle, id: String, data: ScavSaveData) -> Result<SaveResult, String> {
    scav::save_scav(&app, &id, &data)
}

// —— 检视 / 图鉴页（10-inspect）——

/// 检视档案内武器：收集子树并聚合指标（无命令签名暴露 Option）
#[tauri::command]
async fn inspect_weapon(
    app: AppHandle,
    id: String,
    item_id: String,
) -> Result<Option<inspect::BuildView>, String> {
    Ok(inspect::inspect_weapon(&app, &id, &item_id))
}

/// 图鉴：服务器 globals ItemPresets 全局武器预设视图（只读）
#[tauri::command]
async fn builds_global(app: AppHandle) -> Result<Vec<inspect::BuildView>, String> {
    Ok(inspect::builds_global(&app))
}

/// 加入预设：把档案内武器子树存入 userbuilds.weaponBuilds（先备份）
#[tauri::command]
async fn preset_add(app: AppHandle, id: String, item_id: String) -> Result<(), String> {
    inspect::preset_add(&app, &id, &item_id)
}

/// 导出全局预设 JSON 到用户选定路径
#[tauri::command]
async fn preset_export(app: AppHandle, path: String, preset_id: String) -> Result<(), String> {
    inspect::preset_export(&path, &preset_id, &app)
}

/// 导出档案内武器子树 JSON 到用户选定路径
#[tauri::command]
async fn weapon_export(
    app: AppHandle,
    id: String,
    item_id: String,
    path: String,
) -> Result<(), String> {
    inspect::weapon_export(&app, &id, &item_id, &path)
}

/// 预设页数据：武器预设（改装视图）+ 装备预设（13 槽行）
#[tauri::command]
async fn presets_load(app: AppHandle, id: String) -> Option<PresetsData> {
    presets::load_presets(&app, &id)
}

/// 物品图鉴：全量分类树 + 物品行（只读，不碰存档）
#[tauri::command]
async fn encyclopedia_catalog(app: AppHandle) -> Option<encyclopedia::Catalog> {
    encyclopedia::catalog(&app)
}

/// 移除单个预设（先备份）
#[tauri::command]
async fn build_remove(
    app: AppHandle,
    id: String,
    kind: String,
    target: String,
) -> Result<SaveResult, String> {
    presets::preset_remove(&app, &id, &kind, &target)
}

/// 清空某类预设（先备份）
#[tauri::command]
async fn builds_remove_all(
    app: AppHandle,
    id: String,
    kind: String,
) -> Result<SaveResult, String> {
    presets::builds_remove_all(&app, &id, &kind)
}

/// 把预设整树加入仓库（先备份；根走放置引擎）
#[tauri::command]
async fn build_to_stash(
    app: AppHandle,
    id: String,
    kind: String,
    target: String,
) -> Result<SaveResult, String> {
    presets::build_to_stash(&app, &id, &kind, &target)
}

/// 导入预设 JSON（校验失败进列表、重名改名、重编 id；先备份）
#[tauri::command]
async fn build_import(
    app: AppHandle,
    id: String,
    kind: String,
    paths: Vec<String>,
) -> Result<ImportResult, String> {
    presets::build_import(&app, &id, &kind, paths)
}

/// 导出单个预设 JSON 到用户选定路径（原样保留 fastPanel / BuildType）
#[tauri::command]
async fn build_export(
    app: AppHandle,
    id: String,
    kind: String,
    target: String,
    path: String,
) -> Result<(), String> {
    presets::build_export(&app, &id, &kind, &target, &path)
}

/// 导出全部预设 JSON 到用户选定目录，返回成功数
#[tauri::command]
async fn builds_export_all(
    app: AppHandle,
    id: String,
    kind: String,
    dir: String,
) -> Result<usize, String> {
    presets::builds_export_all(&app, &id, &kind, &dir)
}

// —— 问题检测 / 修复（13-issues，保存动作衍生场景）——

/// 问题清单：读档重扫（未选服务器报错；档案缺失或不可读时为空清单）
#[tauri::command]
async fn issues_scan(app: AppHandle, id: String) -> Result<Vec<IssueRow>, String> {
    issues::issues_scan(&app, &id)
}

/// 修复选中问题：先备份后写盘；返回写盘后重扫的剩余问题与备份路径
#[tauri::command]
async fn issues_fix(
    app: AppHandle,
    id: String,
    issue_ids: Vec<String>,
) -> Result<IssuesFixResult, String> {
    issues::issues_fix(&app, &id, &issue_ids)
}

// —— 备份页（14-backup）——

/// 组装当前存档的完整路径（备份三命令共用；is_safe_id 前置校验）
fn profile_path_of(app: &AppHandle, id: &str) -> Result<std::path::PathBuf, String> {
    if !profile::is_safe_id(id) {
        return Err("存档标识不合法".to_string());
    }
    let dir = profile::profiles_dir(app).ok_or_else(|| "未选择 SPT 服务器".to_string())?;
    Ok(std::path::Path::new(&dir).join(format!("{}.json", id)))
}

/// 备份清单：按时间倒序；目录缺失为空清单
#[tauri::command]
async fn backup_list(app: AppHandle, id: String) -> Result<Vec<BackupRow>, String> {
    backup::backup_list(&app, &profile_path_of(&app, &id)?)
}

/// 回档：当前档案先备份 → 选中备份覆盖 → 消耗该备份
#[tauri::command]
async fn backup_restore(app: AppHandle, id: String, backup_id: String) -> Result<(), String> {
    backup::backup_restore(&app, &profile_path_of(&app, &id)?, &backup_id)
}

/// 删除单份备份（不影响当前档案）
#[tauri::command]
async fn backup_delete(app: AppHandle, id: String, backup_id: String) -> Result<(), String> {
    backup::backup_delete(&app, &profile_path_of(&app, &id)?, &backup_id)
}

/// 应用级标志（启动页/引导页首启编排）
#[tauri::command]
async fn app_flags(app: AppHandle) -> Result<server::AppFlags, String> {
    Ok(server::app_flags(&app))
}

/// 标记启动页已看过（下次启动走短版）
#[tauri::command]
async fn mark_splash_seen(app: AppHandle) -> Result<(), String> {
    server::mark_splash_seen(&app)
}

/// 标记引导页已完成（不再自动弹出）
#[tauri::command]
async fn mark_onboarded(app: AppHandle) -> Result<(), String> {
    server::mark_onboarded(&app)
}

// —— 自绘托盘面板（tray.rs）：面板内动作的薄转发 ——

/// 显示主窗口并收起面板
#[tauri::command]
async fn app_show_main(app: AppHandle) {
    tray::show_main_panel(&app);
}

/// 退出应用（唯一真实出口）
#[tauri::command]
async fn app_quit(app: AppHandle) {
    tray::quit(&app);
}

/// 打开当前服务器存档目录（资源管理器）
#[tauri::command]
async fn tray_open_profiles_dir(app: AppHandle) {
    tray::open_profiles_dir(&app);
}

/// 收起面板（Esc / 动作完成）
#[tauri::command]
async fn tray_hide(app: AppHandle) {
    tray::hide_panel(&app);
}

/// 前端上报面板内容高度（逻辑像素），Rust 按锚点贴边重定位
#[tauri::command]
async fn tray_report_height(app: AppHandle, h: f64) {
    tray::report_height(&app, h);
}

/// 读取全部应用设置（15-settings）
#[tauri::command]
async fn settings_get(app: AppHandle) -> Settings {
    settings::get(&app)
}

/// 整体保存设置（改动即时生效并自动保存）
#[tauri::command]
async fn settings_set(app: AppHandle, settings: Settings) -> Result<(), String> {
    settings::set(&app, settings)
}

/// 打开数据文件夹（资源管理器）
#[tauri::command]
async fn settings_open_data_dir(app: AppHandle) -> Result<(), String> {
    settings::open_data_dir(&app)
}

/// 重置设置为默认值（删除 settings.json）
#[tauri::command]
async fn settings_reset(app: AppHandle) -> Result<(), String> {
    settings::reset(&app)
}

/// 重启应用（重置确认后的收尾；never returns）
#[tauri::command]
async fn settings_restart(app: AppHandle) {
    settings::restart(&app);
}

/// 拉取公告：三镜像（raw → 反代 → jsDelivr）→ 本地缓存回退；killSwitch 视为空公告
#[tauri::command]
async fn announcement_fetch(app: AppHandle) -> Result<AnnouncementPayload, String> {
    Ok(announcement::fetch_announcement(&app).await)
}

// ----- 应用更新 -----

/// 检查更新（manual=true 为设置页手动检查；返回终态供设置页 toast）
#[tauri::command]
async fn update_check(
    app: AppHandle,
    manual: Option<bool>,
) -> Result<updater::UpdateStatus, String> {
    updater::check(app, manual.unwrap_or(false)).await
}

/// 开始下载新版本（关窗不中断，「后台下载」语义）
#[tauri::command]
async fn update_download(app: AppHandle) -> Result<(), String> {
    updater::download(app)
}

/// 立即重启安装（退出应用并运行安装器）
#[tauri::command]
async fn update_install(app: AppHandle) -> Result<(), String> {
    updater::install(app)
}

/// 打开更新薄窗（单例）
#[tauri::command]
async fn update_open_window(app: AppHandle) -> Result<(), String> {
    updater::open_window(&app)
}

/// 「稍后提醒」：本会话不再打扰，真关薄窗
#[tauri::command]
async fn update_dismiss(app: AppHandle) -> Result<(), String> {
    updater::dismiss(app)
}

/// 当前更新状态快照（薄窗挂载时先取快照再听广播）
#[tauri::command]
async fn update_get_status() -> updater::UpdateStatus {
    updater::current_status()
}

/// 【dev 专用】广播假更新状态并开薄窗（三态视觉调试）；正式包返回错误，无实际作用
#[tauri::command]
async fn update_dev_mock(app: AppHandle, phase: String) -> Result<(), String> {
    #[cfg(debug_assertions)]
    {
        updater::dev_mock(app, phase)
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = (app, phase);
        Err("仅开发构建可用".into())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            apply_window_rounding(app.handle());
            // conf 的 center:true 对无边框透明窗实测未生效，启动时显式居中兜底；
            // 默认 860 高在小屏会顶出屏幕（实坑），按显示器逻辑高度收敛（预留任务栏等约 48px）
            if let Some(win) = app.get_webview_window("main") {
                if let Ok(Some(m)) = win.current_monitor() {
                    let scale = win.scale_factor().unwrap_or(1.0);
                    let avail = m.size().height as f64 / scale - 48.0;
                    let max_h = avail.clamp(640.0, 860.0);
                    let h = win.inner_size().map(|s| s.height).unwrap_or(0) as f64 / scale;
                    if h > max_h + 1.0 {
                        let _ = win.set_size(tauri::LogicalSize::new(1100.0, max_h));
                    }
                }
                let _ = win.center();
            }
            tray::init(app.handle()).expect("托盘初始化失败");
            // GitHub 会话恢复：凭据库里有 token 则回填内存会话（登录态不依赖前端时序）
            auth::restore();
            // 启动静默检查更新：延迟 3 秒避开启动高峰，受设置开关控制，
            // 失败/无更新零 UI（manual=false 静默语义在 updater::check 内）
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(3));
                if !settings::get(&handle).check_updates {
                    return;
                }
                let _ = tauri::async_runtime::block_on(updater::check(handle, false));
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            match event {
                // 主窗点 X 隐藏到托盘常驻；面板「退出」为唯一真实出口。
                // 更新薄窗除外：真关窗（「后台下载」= 关窗继续下载）
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    if window.label() != updater::UPDATE_WIN {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                }
                // 自绘托盘面板失焦即收起（贴边弹层标准行为）
                tauri::WindowEvent::Focused(false) => {
                    if window.label() == tray::TRAY_WIN {
                        let _ = window.hide();
                    }
                }
                // 更新薄窗销毁：还原开窗时收起的其他可见窗口（专职窗口语义）
                tauri::WindowEvent::Destroyed => {
                    if window.label() == updater::UPDATE_WIN {
                        updater::restore_others(window.app_handle());
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            server_pick, server_add, server_list, server_remove, server_use, server_current,
            profile_list, profile_load, profile_save, profile_current, profile_use,
            char_load, char_options, char_save,
            traders_load, traders_save,
            quests_load, quests_save,
            skills_load, skills_save,
            health_load, health_save,
            hideout_load, hideout_save,
            stash_load, stash_save,
            encyclopedia_catalog,
            scav_load, scav_save,
            inspect_weapon, builds_global, preset_add, preset_export, weapon_export,
            presets_load, build_remove, builds_remove_all, build_to_stash,
            build_import, build_export, builds_export_all,
            issues_scan, issues_fix,
            backup_list, backup_restore, backup_delete,
            app_flags, mark_splash_seen, mark_onboarded, announcement_fetch,
            settings_get, settings_set, settings_open_data_dir, settings_reset, settings_restart,
            ai::ai_get_config, ai::ai_set_config, ai::ai_set_key, ai::ai_clear_key, ai::ai_detect_ollama,
            ai::ai_test_connection, ai::ai_chat,
            clothing::clothing_load, clothing::clothing_save,
            arsenal::arsenal_load, arsenal::arsenal_apply,
            assistant::assistant_context, assistant::assistant_execute,
            app_show_main, app_quit, tray_open_profiles_dir, tray_hide, tray_report_height,
            auth_is_configured, auth_device_start, auth_device_cancel, auth_logout,
            sync_pull, sync_push,
            update_check, update_download, update_install, update_open_window, update_dismiss,
            update_get_status, update_dev_mock
        ])
        .run(tauri::generate_context!())
        .expect("Phoenix-DRX 启动失败");
}

/// 无边框透明窗口请求 DWM 圆角（Win11 一致；透明窗口下视觉圆角仍由前端
/// CSS 半径提供，此处为系统级补充）。样板 tengri-qingli 同款实现。
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
