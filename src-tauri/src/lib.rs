mod commands;
mod domain;
mod error;
mod features;
mod infrastructure;
mod state;
mod tools;

use commands::*;
use features::{game_session::start_game_monitor, obs::start_obs_monitor};
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
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            app.manage(AppState::new(&app_data_dir)?);
            let state = app.state::<AppState>();
            let local_analysis = state.local_analysis.clone();
            tauri::async_runtime::spawn_blocking(move || local_analysis.load_cached_indexes());
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
                        if let Some(window) = tray.app_handle().get_webview_window("main") {
                            let _ = window.unminimize();
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
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
                        && let Some(window) = tray.app_handle().get_webview_window("main")
                    {
                        let _ = window.unminimize();
                        let _ = window.show();
                        let _ = window.set_focus();
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
                features::tosu::cleanup_on_exit(&app.state::<AppState>().tosu);
            }
        });
}
