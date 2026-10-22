//! OPP 后端应用入口：组装 Tauri、共享状态、后台监视器与命令注册。

mod commands;
mod domain;
mod error;
mod features;
mod infrastructure;
mod state;
mod tools;

use commands::*;
use features::{game_session::start_game_monitor, obs::start_obs_monitor};
use infrastructure::logging;
use infrastructure::portable_update;
use state::AppState;
use tauri::{
    Manager,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

/// Linux 专属：把主窗口收缩到显示器工作区的 90% 以内并重新居中，
/// 规避 X11 窗口管理器把超工作区的无边框窗口强制最大化。
#[cfg(target_os = "linux")]
fn fit_linux_main_window(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    let Some(monitor) = window.current_monitor()?.or(window.primary_monitor()?) else {
        return Ok(());
    };
    let scale = monitor.scale_factor();
    let work_area = monitor.work_area();
    let logical = work_area.size.to_logical::<f64>(scale);

    // X11 窗口管理器常会把大于工作区的无边框窗口直接最大化。
    // 默认 1440×900 在 1366×768 或小数缩放后的 1080p 工作区中会
    // 触发该行为，因此按工作区保留 10% 边距，同时保留原默认上限。
    let width = 1440.0_f64.min((logical.width * 0.9).floor()).max(1.0);
    let height = 900.0_f64.min((logical.height * 0.9).floor()).max(1.0);
    window.set_min_size(Some(tauri::LogicalSize::new(
        1120.0_f64.min(width),
        720.0_f64.min(height),
    )))?;
    window.set_fullscreen(false)?;
    window.unmaximize()?;
    window.set_size(tauri::LogicalSize::new(width, height))?;

    let physical_width = (width * scale).round() as i32;
    let physical_height = (height * scale).round() as i32;
    let x = work_area.position.x + (work_area.size.width as i32 - physical_width) / 2;
    let y = work_area.position.y + (work_area.size.height as i32 - physical_height) / 2;
    window.set_position(tauri::PhysicalPosition::new(x, y))?;
    Ok(())
}

/// 便携版更新助手入口：进程以更新助手参数启动时仅执行文件替换并返回 `true`，
/// 调用方应直接返回，不再初始化窗口、托盘与插件进入正常应用流程。
pub fn run_portable_update_helper_if_requested() -> bool {
    portable_update::run_helper_if_requested()
}

/// 应用主入口：注册插件与命令、初始化日志与共享状态、拉起后台监视器与托盘；
/// 退出事件中按顺序释放本地数据库、音乐运行时、tosu 与日志资源。
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 须在任何子进程（opener 插件、danser、tosu、ffmpeg）派生前剥离 AppImage
    // 泄漏给子进程的加载器变量，否则 OAuth 打开浏览器等操作可能崩溃（issue #51）。
    infrastructure::platform::strip_appimage_loader_env();
    tauri::Builder::default()
        // opp:// 深链的待确认赛事链接队列，需在深链插件初始化之前完成托管。
        .manage(features::tournament_pools::uri::TournamentLinks::default())
        // 单实例守卫：二次启动时唤起已激活窗口；`opp:` 深链参数交由深链插件处理。
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // Deep-link forwarding has already run. Its handler restores the main window.
            if !argv.iter().any(|arg| arg.starts_with("opp:"))
                && app.try_state::<AppState>().is_some()
            {
                features::music_player::windows::show_active(app);
            }
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            let logger = logging::init(&app_data_dir);
            logger.log(
                "INFO",
                "app.lifecycle",
                format!("OPP {} started", env!("CARGO_PKG_VERSION")),
            );
            app.manage(AppState::new(&app_data_dir)?);
            app.manage(logger);
            features::tournament_pools::uri::initialize(app.handle());
            let state = app.state::<AppState>();
            // Windows：创建隐藏宿主窗口承接系统媒体集成；失败时降级为
            // 应用内播放控制，不阻断启动。
            #[cfg(windows)]
            let media_hwnd = {
                let host = tauri::WindowBuilder::new(app, "music-media-host")
                    .title("OPP music media host")
                    .visible(false)
                    .skip_taskbar(true)
                    .build()
                    .and_then(|host| host.hwnd().map(|hwnd| hwnd.0 as usize));
                match host {
                    Ok(hwnd) => Some(hwnd),
                    Err(error) => {
                        crate::log_warn!(
                            "music_player",
                            "无法创建系统媒体宿主，保留窗口播放控制：{}",
                            error
                        );
                        None
                    }
                }
            };
            #[cfg(not(windows))]
            let media_hwnd = None;
            state.music.start(app.handle().clone(), media_hwnd);
            let local_analysis = state.local_analysis.clone();
            let music_app = app.handle().clone();
            // 本地索引恢复与音乐队列初始化放后台执行，避免阻塞应用启动。
            tauri::async_runtime::spawn(async move {
                let loading = local_analysis.clone();
                let cache_loader = loading.clone();
                let _ =
                    crate::infrastructure::tasks::background("load_cached_indexes", move || {
                        cache_loader.load_cached_indexes()
                    })
                    .await;
                // Restore the persisted indexes before starting filesystem watchers.
                // Starting a watcher while the cache is still loading can race with
                // the first Lazer/Stable filesystem event and schedule a full scan
                // against an empty in-memory index on every application launch.
                loading.start_watchers(music_app.clone());
                music_app
                    .state::<AppState>()
                    .music
                    .initialize_queue(local_analysis)
                    .await;
            });
            start_game_monitor(
                state.local_analysis.clone(),
                state.game_monitor.clone(),
                app.handle().clone(),
            );
            start_obs_monitor(app.handle().clone());
            features::overlay::service::initialize(app.handle());
            let icon = tauri::image::Image::from_bytes(include_bytes!("../../public/01.png"))?;
            if let Some(window) = app.get_webview_window("main") {
                #[cfg(target_os = "linux")]
                if let Err(error) = fit_linux_main_window(&window) {
                    crate::log_warn!("app.lifecycle", "无法按 Linux 工作区调整主窗口：{}", error);
                }
                window.set_icon(icon.clone())?;
            }
            let show_window =
                MenuItem::with_id(app, "show-window", "显示界面", true, None::<&str>)?;
            let exit = MenuItem::with_id(app, "exit", "退出", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&show_window, &exit])?;
            TrayIconBuilder::with_id("opp-tray")
                .icon(icon)
                .tooltip("OPP")
                .menu(&tray_menu)
                .on_menu_event(|tray, event| match event.id().as_ref() {
                    "show-window" => {
                        features::music_player::windows::show_active(tray.app_handle());
                    }
                    "exit" => tray.app_handle().exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        features::music_player::windows::show_active(tray.app_handle());
                    }
                })
                .build(app)?;
            // 清理上一轮更新流程可能遗留的暂存文件。
            portable_update::schedule_stale_cleanup();
            Ok(())
        })
        .invoke_handler(commands::handler!())
        .build(tauri::generate_context!())
        .expect("failed to build OPP")
        .run(|app, event| {
            // 退出事件：按依赖顺序释放各运行时，最后关闭日志系统。
            if let tauri::RunEvent::Exit = event {
                app.state::<AppState>().local_database.shutdown();
                app.state::<AppState>().music.shutdown();
                features::tosu::cleanup_on_exit(&app.state::<AppState>().tosu);
                logging::shutdown();
            }
        });
}
