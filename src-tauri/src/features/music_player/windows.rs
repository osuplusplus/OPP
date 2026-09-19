use super::{
    models::WindowSession,
    service::{failure, write_json},
};
use crate::{error::CommandResult, state::AppState};
use std::sync::atomic::Ordering;
use tauri::{Emitter, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

pub(crate) fn background_tasks(state: &AppState) -> usize {
    state.local_analysis.music_background_tasks()
        + usize::from(state.beatmap_download.lock().is_ok_and(|v| v.is_some()))
        + usize::from(state.danser.is_busy())
        + usize::from(state.music_frontend_task.load(Ordering::Relaxed))
        + usize::from(
            state
                .game_monitor
                .current
                .lock()
                .is_ok_and(|s| s.clients.iter().any(|c| c.running)),
        )
        + usize::from(state.obs.current.lock().is_ok_and(|s| s.running))
        + usize::from(crate::features::live_render::is_busy())
}

pub fn show_active(app: &tauri::AppHandle) {
    let label = if app
        .state::<AppState>()
        .music
        .snapshot()
        .is_ok_and(|s| s.mini)
    {
        "mini-player"
    } else {
        "main"
    };
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

pub(crate) fn constrain(
    window: &tauri::WebviewWindow,
    x: Option<i32>,
    y: Option<i32>,
    height: f64,
) -> CommandResult<()> {
    let monitors = window.available_monitors()?;
    let requested = x.zip(y);
    let monitor = requested
        .and_then(|(x, y)| {
            monitors.iter().find(|m| {
                let p = m.position();
                let size = m.size();
                x >= p.x && y >= p.y && x < p.x + size.width as i32 && y < p.y + size.height as i32
            })
        })
        .or(monitors.first());
    if let Some(monitor) = monitor {
        let scale = monitor.scale_factor();
        let area = monitor.work_area();
        let width = (360.0 * scale).min(f64::from(area.size.width));
        let height = (height * scale).min(f64::from(area.size.height));
        window.set_size(tauri::PhysicalSize::new(width as u32, height as u32))?;
        let (x, y) = requested.unwrap_or((
            area.position.x + area.size.width as i32 - width as i32 - 24,
            area.position.y + 24,
        ));
        window.set_position(PhysicalPosition::new(
            x.clamp(
                area.position.x,
                area.position.x + area.size.width as i32 - width as i32,
            ),
            y.clamp(
                area.position.y,
                area.position.y + area.size.height as i32 - height as i32,
            ),
        ))?;
    }
    Ok(())
}

pub(crate) fn switch(
    app: &tauri::AppHandle,
    mini: bool,
    route: Option<String>,
) -> CommandResult<()> {
    let state = app.state::<AppState>();
    let target = if mini { "mini-player" } else { "main" };
    let token = format!("{target}:{}", uuid::Uuid::new_v4());
    {
        let mut pending = state
            .music
            .switching
            .lock()
            .map_err(|_| failure("窗口状态不可用"))?;
        if pending.is_some() {
            return Err(failure("正在切换窗口，请稍候"));
        }
        if state.music.snapshot()?.mini == mini {
            show_active(app);
            return Ok(());
        }
        *pending = Some(token.clone());
    }
    let result = (|| {
        let mut session = state
            .music
            .session
            .lock()
            .map_err(|_| failure("窗口状态不可用"))?
            .clone();
        if mini {
            if let Some(main) = app.get_webview_window("main") {
                let size = main.inner_size()?.to_logical::<f64>(main.scale_factor()?);
                let position = main.outer_position()?;
                session.width = Some(size.width);
                session.height = Some(size.height);
                session.x = Some(position.x);
                session.y = Some(position.y);
                session.maximized = main.is_maximized()?;
            }
            if let Some(route) = route.filter(|r| r.starts_with('/') && r.len() < 4096) {
                session.route = route;
            }
            let builder = WebviewWindowBuilder::new(
                app,
                "mini-player",
                WebviewUrl::App("mini-player.html".into()),
            )
            .title("OPP · 本地音乐")
            .inner_size(360.0, 72.0)
            .transparent(true)
            .decorations(false)
            .resizable(false)
            .always_on_top(session.pinned.unwrap_or(true))
            .visible(false);
            // A distinct WebView2 environment lets the full browser process and its GPU caches exit.
            // Linux deliberately reuses its context: WebKit's network process outlives destroyed contexts.
            #[cfg(windows)]
            let builder = builder.data_directory(state.music.directory.join("mini-webview"));
            let window = builder.build()?;
            constrain(&window, session.mini_x, session.mini_y, 72.0)?;
            let handle = app.clone();
            window.on_window_event(move |event| match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    if let Some(w) = handle.get_webview_window("mini-player") {
                        let _ = w.hide();
                    }
                }
                tauri::WindowEvent::Moved(position) => {
                    if let Ok(mut session) = handle.state::<AppState>().music.session.lock() {
                        session.mini_x = Some(position.x);
                        session.mini_y = Some(position.y);
                    }
                }
                _ => {}
            });
        } else {
            if let Some(main) = app.get_webview_window("main") {
                main.show()?;
                main.set_focus()?;
                state.music.state.lock().unwrap().mini = false;
                state.music_only.store(false, Ordering::Relaxed);
                state.local_analysis.set_music_only(false);
                if let Some(mini) = app.get_webview_window("mini-player") {
                    mini.destroy()?;
                }
                *state.music.switching.lock().unwrap() = None;
                return Ok(());
            }
            state.local_analysis.set_music_only(false);
            // Loading is performed by the async command before this UI-thread operation.
            let config = app
                .config()
                .app
                .windows
                .iter()
                .find(|w| w.label == "main")
                .ok_or_else(|| failure("主窗口配置不存在"))?;
            let script = format!(
                "window.location.hash = {};",
                serde_json::to_string(&session.route)?
            );
            let window = WebviewWindowBuilder::from_config(app, config)?
                .visible(false)
                .initialization_script(script)
                .build()?;
            window.set_size(LogicalSize::new(
                session.width.unwrap_or(1440.0).max(1120.0),
                session.height.unwrap_or(900.0).max(720.0),
            ))?;
            if let Some((x, y)) = session.x.zip(session.y) {
                let on_screen = window.available_monitors()?.iter().any(|m| {
                    x >= m.position().x
                        && y >= m.position().y
                        && x < m.position().x + m.size().width as i32
                        && y < m.position().y + m.size().height as i32
                });
                if on_screen {
                    window.set_position(PhysicalPosition::new(x, y))?;
                } else {
                    window.center()?;
                }
            }
            if session.maximized {
                window.maximize()?;
            }
        }
        *state.music.session.lock().unwrap() = session.clone();
        write_json(&state.music.directory.join("window.json"), &session)?;
        Ok(())
    })();
    if result.is_err() {
        if let Some(window) = app.get_webview_window(target) {
            let _ = window.destroy();
        }
        *state.music.switching.lock().unwrap() = None;
        state
            .local_analysis
            .set_music_only(state.music.snapshot()?.mini);
        return result;
    }
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(15)).await;
        let state = handle.state::<AppState>();
        let expired = {
            let mut pending = state.music.switching.lock().unwrap();
            if pending.as_ref() == Some(&token) {
                *pending = None;
                true
            } else {
                false
            }
        };
        if expired {
            if let Some(window) = handle.get_webview_window(target) {
                let _ = window.destroy();
            }
            state
                .local_analysis
                .set_music_only(state.music.snapshot().is_ok_and(|s| s.mini));
            crate::log_warn!("music_player", "窗口切换超时，保留原窗口");
            let _ = handle.emit("music-window-error", "窗口加载超时，已保留原窗口");
        }
    });
    Ok(())
}

pub(crate) fn ready(app: &tauri::AppHandle, label: &str) -> CommandResult<WindowSession> {
    let state = app.state::<AppState>();
    let target_matches = state
        .music
        .switching
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|s| s.starts_with(&format!("{label}:")));
    if target_matches {
        let mini = label == "mini-player";
        let new = app
            .get_webview_window(label)
            .ok_or_else(|| failure("目标窗口不存在"))?;
        new.show()?;
        new.set_focus()?;
        let old = if mini { "main" } else { "mini-player" };
        if let Some(window) = app.get_webview_window(old) {
            if mini && background_tasks(&state) > 0 {
                window.hide()?;
            } else {
                window.destroy()?;
            }
        }
        {
            let mut snapshot = state.music.state.lock().unwrap();
            snapshot.mini = mini;
            snapshot.version += 1;
        }
        state.music_only.store(mini, Ordering::Relaxed);
        state.local_analysis.set_music_only(mini);
        if mini {
            if background_tasks(&state) == 0 {
                state.local_analysis.release_music_idle_indexes();
            }
            state.similarity.clear(crate::domain::Ruleset::Osu);
            state.similarity.clear(crate::domain::Ruleset::Mania);
        }
        *state.music.switching.lock().unwrap() = None;
        let _ = app.emit("music-player-state", state.music.snapshot()?);
    }
    Ok(state.music.session.lock().unwrap().clone())
}

/// Frontend-orchestrated tasks retain their hidden owner only until their final step has completed.
pub(crate) fn release_completed_window(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    if state.music.snapshot().is_ok_and(|s| s.mini)
        && background_tasks(&state) == 0
        && state
            .music
            .switching
            .lock()
            .is_ok_and(|pending| pending.is_none())
        && let Some(main) = app.get_webview_window("main")
    {
        let _ = main.destroy();
    }
}
