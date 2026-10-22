//! 窗口集成：统计后台任务、主窗口与迷你播放器的切换流程、
//! 迷你窗口的显示区域约束，以及窗口会话的持久化。
use super::{
    models::WindowSession,
    service::{failure, write_json},
};
use crate::{error::CommandResult, state::AppState};
use std::sync::atomic::Ordering;
use tauri::{Emitter, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

/// 汇总仍在占用资源的后台任务数：音乐索引、谱面下载、danser 渲染、前端任务、
/// 游戏客户端监控、OBS 推流与直播渲染。迷你模式据此决定何时释放索引、
/// 回收隐藏的主窗口。
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

/// 按 `mini` 标志唤起并聚焦迷你播放器或主窗口（供媒体面板的 Raise 事件使用）。
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

/// 把迷你窗口限制在目标显示器的工作区内：宽度固定 360 逻辑像素（按缩放比换算），
/// 未指定坐标时停靠在显示器右上角，越界坐标一律夹回工作区。
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

/// 在主窗口与迷你播放器之间切换。
///
/// 通过 `switching` 中的令牌防止并发切换；新窗口就绪前旧窗口保持原状，
/// 15 秒内未收到就绪回调（`ready`）则看门狗销毁新窗口、保留原窗口。
pub(crate) fn switch(
    app: &tauri::AppHandle,
    mini: bool,
    route: Option<String>,
) -> CommandResult<()> {
    let state = app.state::<AppState>();
    let target = if mini { "mini-player" } else { "main" };
    // 令牌绑定目标窗口与本次切换，供就绪回调与超时看门狗核对。
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
            // 先把主窗口的几何与会话保存起来，切回时按原样恢复。
            if let Some(main) = app.get_webview_window("main") {
                let size = main.inner_size()?.to_logical::<f64>(main.scale_factor()?);
                let position = main.outer_position()?;
                session.width = Some(size.width);
                session.height = Some(size.height);
                session.x = Some(position.x);
                session.y = Some(position.y);
                session.maximized = main.is_maximized()?;
            }
            // 仅接受站内路由（以 “/” 开头且长度受限）才写入会话。
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
            // 关闭请求只隐藏迷你窗口（保留实例便于再次唤起）；移动时把位置记入会话。
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
            // 主窗口仍存活：直接显示、恢复完整模式并销毁迷你窗口。
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
            // 主窗口此前已被回收：按 tauri 配置重建，并用初始化脚本恢复上次路由。
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
            // 坐标仍落在某个显示器内才恢复，否则居中（避免拔掉显示器后窗口找不回）。
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
    // 切换失败：销毁半成品窗口，并按当前 mini 状态恢复分析模式。
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
    // 超时看门狗：新窗口 15 秒内未上报就绪则回收它并保留原窗口。
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

/// 新窗口前端加载完成后由前端调用：核对切换令牌、显示新窗口并处置旧窗口。
///
/// 迷你模式下仍有后台任务时主窗口仅隐藏（结束后由 `release_completed_window`
/// 回收），否则直接销毁；进入迷你模式还会释放空闲索引与相似谱面缓存。
/// 返回窗口会话供前端恢复布局。
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

/// 迷你模式下所有后台任务结束且无切换进行时，销毁隐藏的主窗口以释放资源。
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
