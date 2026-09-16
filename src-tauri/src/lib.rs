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

pub fn run_portable_update_helper_if_requested() -> bool {
    portable_update::run_helper_if_requested()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            features::music_player::windows::show_active(app);
        }))
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
            let state = app.state::<AppState>();
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
            tauri::async_runtime::spawn(async move {
                let loading = local_analysis.clone();
                let _ = tauri::async_runtime::spawn_blocking(move || loading.load_cached_indexes())
                    .await;
                music_app
                    .state::<AppState>()
                    .music
                    .initialize_queue(local_analysis)
                    .await;
            });
            state.local_analysis.start_watchers(app.handle().clone());
            let beatmaphub = state.beatmaphub.clone();
            tauri::async_runtime::spawn(async move {
                let _ = beatmaphub.recommendations(20, true).await;
            });
            start_game_monitor(
                state.local_analysis.clone(),
                state.game_monitor.clone(),
                app.handle().clone(),
            );
            start_obs_monitor(app.handle().clone());
            let icon = tauri::image::Image::from_bytes(include_bytes!("../../public/01.png"))?;
            if let Some(window) = app.get_webview_window("main") {
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
            portable_update::schedule_stale_cleanup();
            Ok(())
        })
        .invoke_handler(commands::handler!())
        .build(tauri::generate_context!())
        .expect("failed to build OPP")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                app.state::<AppState>().music.shutdown();
                features::tosu::cleanup_on_exit(&app.state::<AppState>().tosu);
            }
        });
}
