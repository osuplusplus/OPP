mod account;
mod collections;
mod danser;
mod error;
mod game_session;
mod local_analysis;
mod models;
mod netease_music;
mod obs;
mod online_beatmaps;
mod osu_api;
mod platform;
mod pp_calc;
mod replay_render;
mod similarity;
mod skin_workshop;
mod state;
mod storage;
mod tools;
mod tosu;
mod trainer;
mod update_check;

use account::{
    begin_oauth_login, cancel_oauth_login, clear_profile_cache, disconnect_osu,
    export_replay_video, get_auth_status, get_own_profile, get_scores, get_settings,
    mark_onboarding_seen, mark_page_onboarding_seen, save_oauth_credentials, update_settings,
};
use collections::{
    add_collection_entries, begin_collection_task, cancel_collection_task, create_collection,
    delete_collection, export_collection_share, get_collection_download_items,
    get_collection_sync_status, import_collection_share, install_collection_downloads,
    list_collections, open_collection_downloads, preview_collection_share, refresh_collections,
    remove_collection_entry, rename_collection, write_stable_collections,
};
use danser::{
    cancel_danser_render, enqueue_danser_renders, get_danser_render_queue, get_danser_status,
    list_danser_profiles, open_danser_output, start_danser_render_queue,
};
use game_session::{
    get_game_session_status, get_game_status, inspect_game_replay, list_game_media,
    open_media_in_explorer, read_game_replay, read_game_screenshot, start_detected_game_session,
    start_game_monitor, start_game_session,
};
use local_analysis::{
    cancel_local_scan, export_local_beatmap_set, export_local_skin, get_local_beatmap_background,
    get_local_beatmap_detail, get_local_beatmap_path, get_local_index_status, get_local_skin_asset,
    get_local_skin_detail, get_local_skin_preview, get_local_sources, get_local_summary,
    query_local_beatmap_sets, query_local_beatmaps, query_local_skins, replace_local_skin_asset,
    reset_local_source, scan_local_source, set_local_source,
};
use netease_music::open_netease_music_search;
use obs::{
    get_obs_scenes, get_obs_status, refresh_selected_obs_scene, save_obs_connection,
    start_obs_monitor,
};
use online_beatmaps::{
    cancel_online_beatmap_download, collect_online_beatmapsets, download_online_beatmapsets,
    get_online_beatmap, get_online_beatmap_provider_status, get_online_beatmapset,
    open_downloaded_path, search_online_beatmapsets,
};
use platform::get_capabilities;
use pp_calc::calculate_beatmap_pp;
use replay_render::submit_replay_render;
use similarity::{
    configure_similarity_index, get_similarity_index_status, query_similar_beatmaps,
    recommend_similar_beatmaps,
};
use skin_workshop::{
    execute_skin_workshop_action, execute_skin_workshop_preset, get_skin_workshop_asset,
    get_skin_workshop_config, get_skin_workshop_part_preview, get_skin_workshop_tree,
    open_skin_workshop_package,
};
use state::AppState;
use tauri::{
    Manager,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tools::{
    cancel_lazer_dedupe, convert_mania_beatmaps, dedupe_lazer_files, generate_beatmap_preview,
    get_default_file_clients, get_lazer_disk_usage, inspect_beatmap_preview,
    open_beatmap_preview_output, open_local_resource_in_explorer, read_beatmap_preview_output,
    read_lazer_realm_beatmap_sets, save_beatmap_preview_output, set_default_file_client,
    set_display_gamma,
};
use tosu::{
    get_tosu_logs, get_tosu_status, set_tosu_executable, set_tosu_lyrics_executable, start_tosu,
    stop_tosu,
};
use trainer::generate_trainer_beatmap;
use update_check::{check_for_updates, ignore_update_version};

#[tauri::command]
fn exit_app(app: tauri::AppHandle) {
    app.exit(0);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            app.manage(AppState::new(&app_data_dir)?);
            let state = app.state::<AppState>();
            let local_analysis = state.local_analysis.clone();
            tauri::async_runtime::spawn_blocking(move || local_analysis.load_cached_indexes());
            start_game_monitor(
                state.local_analysis.clone(),
                state.game_monitor.clone(),
                app.handle().clone(),
            );
            start_obs_monitor(app.handle().clone());
            let icon = app
                .default_window_icon()
                .expect("application bundle must include an icon")
                .clone();
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
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_auth_status,
            get_capabilities,
            list_collections,
            get_collection_sync_status,
            refresh_collections,
            create_collection,
            rename_collection,
            delete_collection,
            add_collection_entries,
            remove_collection_entry,
            write_stable_collections,
            export_collection_share,
            preview_collection_share,
            import_collection_share,
            get_collection_download_items,
            begin_collection_task,
            cancel_collection_task,
            install_collection_downloads,
            open_collection_downloads,
            save_oauth_credentials,
            begin_oauth_login,
            cancel_oauth_login,
            disconnect_osu,
            get_own_profile,
            get_scores,
            search_online_beatmapsets,
            collect_online_beatmapsets,
            get_online_beatmapset,
            get_online_beatmap,
            get_online_beatmap_provider_status,
            calculate_beatmap_pp,
            submit_replay_render,
            get_danser_status,
            list_danser_profiles,
            enqueue_danser_renders,
            start_danser_render_queue,
            get_danser_render_queue,
            cancel_danser_render,
            open_danser_output,
            get_similarity_index_status,
            configure_similarity_index,
            query_similar_beatmaps,
            recommend_similar_beatmaps,
            start_game_session,
            start_detected_game_session,
            get_game_status,
            get_game_session_status,
            list_game_media,
            read_game_replay,
            inspect_game_replay,
            read_game_screenshot,
            open_media_in_explorer,
            download_online_beatmapsets,
            cancel_online_beatmap_download,
            open_downloaded_path,
            clear_profile_cache,
            get_settings,
            mark_onboarding_seen,
            mark_page_onboarding_seen,
            update_settings,
            export_replay_video,
            get_local_sources,
            get_local_index_status,
            set_local_source,
            reset_local_source,
            get_local_summary,
            scan_local_source,
            export_local_beatmap_set,
            export_local_skin,
            cancel_local_scan,
            query_local_beatmaps,
            query_local_beatmap_sets,
            get_local_beatmap_detail,
            get_local_beatmap_path,
            get_local_beatmap_background,
            query_local_skins,
            get_local_skin_detail,
            get_local_skin_preview,
            get_local_skin_asset,
            replace_local_skin_asset,
            open_skin_workshop_package,
            execute_skin_workshop_action,
            execute_skin_workshop_preset,
            get_skin_workshop_tree,
            get_skin_workshop_part_preview,
            get_skin_workshop_asset,
            get_skin_workshop_config,
            open_local_resource_in_explorer,
            get_default_file_clients,
            set_default_file_client,
            set_display_gamma,
            get_lazer_disk_usage,
            read_lazer_realm_beatmap_sets,
            dedupe_lazer_files,
            cancel_lazer_dedupe,
            open_netease_music_search,
            convert_mania_beatmaps,
            inspect_beatmap_preview,
            generate_beatmap_preview,
            read_beatmap_preview_output,
            save_beatmap_preview_output,
            open_beatmap_preview_output,
            generate_trainer_beatmap,
            get_tosu_status,
            get_tosu_logs,
            set_tosu_executable,
            set_tosu_lyrics_executable,
            start_tosu,
            stop_tosu,
            get_obs_status,
            get_obs_scenes,
            save_obs_connection,
            refresh_selected_obs_scene,
            check_for_updates,
            ignore_update_version,
            exit_app,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build OPP")
        .run(|app, event| {
            // 退出时终止 OPP 自己启动的 tosu-lyrics 子进程，避免孤儿代理残留。
            if let tauri::RunEvent::Exit = event {
                tosu::cleanup_on_exit(&app.state::<AppState>().tosu);
            }
        });
}
