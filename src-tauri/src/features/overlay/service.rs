//! 覆盖层窗口生命周期、模式切换、热键与游戏状态钩子。

use std::sync::Mutex;

use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, Monitor, PhysicalPosition, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

use crate::{
    domain::AppSettings,
    error::{CommandError, CommandResult},
    state::AppState,
};

/// 覆盖层窗口标签；同一时刻至多存在一个覆盖层窗口。
pub const OVERLAY_WINDOW_LABEL: &str = "overlay";
/// 模式切换后广播给前端的事件名（载荷为 `"hud"` / `"panel"`）。
const OVERLAY_MODE_EVENT: &str = "overlay-mode-changed";

/// HUD 卡片与面板侧栏的逻辑尺寸（CSS px），实际渲染随系统 DPI 缩放。
const HUD_SIZE: (f64, f64) = (340.0, 190.0);
/// Direct 面板侧栏的逻辑尺寸（会按显示器工作区收缩）。
const PANEL_SIZE: (f64, f64) = (460.0, 800.0);
/// 窗口与屏幕边缘的逻辑间距。
const CORNER_MARGIN: f64 = 16.0;

/// 覆盖层形态：HUD 为点击穿透的状态卡片，Panel 为可交互的 Direct 侧栏。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayMode {
    Hud,
    Panel,
}

impl OverlayMode {
    pub fn as_str(self) -> &'static str {
        match self {
            OverlayMode::Hud => "hud",
            OverlayMode::Panel => "panel",
        }
    }

    pub fn parse(value: &str) -> CommandResult<Self> {
        match value {
            "hud" => Ok(OverlayMode::Hud),
            "panel" => Ok(OverlayMode::Panel),
            other => Err(CommandError::new(
                "INVALID_OVERLAY_MODE",
                format!("未知的覆盖层模式：{other}"),
            )),
        }
    }
}

// 进程内只有一个覆盖层窗口，用静态量记录当前模式即可。
static CURRENT_MODE: Mutex<OverlayMode> = Mutex::new(OverlayMode::Hud);

fn current_mode() -> OverlayMode {
    CURRENT_MODE
        .lock()
        .map(|mode| *mode)
        .unwrap_or(OverlayMode::Hud)
}

/// 当前模式的字符串表示（命令层序列化用）。
pub fn current_mode_str() -> String {
    current_mode().as_str().to_owned()
}

/// 读取设置快照；存储读取失败时回退默认值，覆盖层不因此中断。
fn settings_snapshot(app: &AppHandle) -> AppSettings {
    app.state::<AppState>()
        .store
        .settings_snapshot()
        .unwrap_or_default()
}

/// 屏幕四角锚点，决定覆盖层在工作区内的停靠角。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Corner {
    /// 解析设置字符串；未知取值回退右上角。
    fn parse(value: &str) -> Self {
        match value {
            "TopLeft" => Corner::TopLeft,
            "BottomLeft" => Corner::BottomLeft,
            "BottomRight" => Corner::BottomRight,
            _ => Corner::TopRight,
        }
    }
}

/// 面板尺寸按所在显示器工作区收缩，避免小屏溢出。
fn panel_size_on(monitor: &Monitor) -> (f64, f64) {
    let scale = monitor.scale_factor();
    let area = monitor.work_area().size.to_logical::<f64>(scale);
    (
        PANEL_SIZE
            .0
            .min((area.width - CORNER_MARGIN * 2.0).max(280.0))
            .max(280.0),
        PANEL_SIZE
            .1
            .min((area.height - CORNER_MARGIN * 2.0).max(320.0))
            .max(320.0),
    )
}

/// 按锚点计算停靠位置：逻辑尺寸换算为物理像素，基于工作区（避开任务栏）。
fn anchored_position(
    monitor: &Monitor,
    corner: Corner,
    logical_size: (f64, f64),
) -> PhysicalPosition<i32> {
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let width = (logical_size.0 * scale).round() as i32;
    let height = (logical_size.1 * scale).round() as i32;
    let margin = (CORNER_MARGIN * scale).round() as i32;
    let x = match corner {
        Corner::TopLeft | Corner::BottomLeft => area.position.x + margin,
        Corner::TopRight | Corner::BottomRight => {
            area.position.x + area.size.width as i32 - width - margin
        }
    };
    let y = match corner {
        Corner::TopLeft | Corner::TopRight => area.position.y + margin,
        Corner::BottomLeft | Corner::BottomRight => {
            area.position.y + area.size.height as i32 - height - margin
        }
    };
    PhysicalPosition::new(x, y)
}

/// 取窗口当前所在显示器，查询失败时回退主显示器。
fn overlay_monitor(window: &WebviewWindow) -> Option<Monitor> {
    window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())
}

/// 创建覆盖层窗口：透明、无边框、置顶、跳过任务栏且初始不可见；创建后
/// 立即把关闭请求改写为隐藏以保活 WebView。
fn create_overlay_window(app: &AppHandle) -> CommandResult<WebviewWindow> {
    let builder = WebviewWindowBuilder::new(
        app,
        OVERLAY_WINDOW_LABEL,
        WebviewUrl::App("overlay.html".into()),
    )
    .title("OPP · 游戏覆盖")
    .inner_size(HUD_SIZE.0, HUD_SIZE.1)
    .transparent(true)
    .decorations(false)
    .resizable(false)
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .focused(false)
    .visible(false);
    // Windows 上给覆盖层独立 WebView2 环境（同 mini-player：进程与 GPU
    // 缓存独立，销毁窗口时能干净退出）。Linux 复用上下文。
    #[cfg(windows)]
    let builder = {
        let data_dir = app
            .path()
            .app_data_dir()
            .map_err(|error| CommandError::new("OVERLAY_DATA_DIR", format!("{error}")))?
            .join("overlay-webview");
        builder.data_directory(data_dir)
    };
    let window = builder
        .build()
        .map_err(|error| CommandError::new("OVERLAY_WINDOW_FAILED", format!("{error}")))?;
    let handle = app.clone();
    window.on_window_event(move |event| {
        // 关闭请求一律转为隐藏：WebView 保活才能维持 MP 自动补缺监听。
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            if let Some(window) = handle.get_webview_window(OVERLAY_WINDOW_LABEL) {
                let _ = window.hide();
            }
        }
    });
    Ok(window)
}

/// 确保覆盖层窗口存在（不改变可见性）。
pub fn ensure_overlay_window(app: &AppHandle) -> CommandResult<WebviewWindow> {
    if let Some(existing) = app.get_webview_window(OVERLAY_WINDOW_LABEL) {
        return Ok(existing);
    }
    let window = create_overlay_window(app)?;
    apply_mode(app, OverlayMode::Hud)?;
    Ok(window)
}

/// 应用模式：尺寸、角落锚点、点击穿透与焦点。窗口须已存在。
fn apply_mode(app: &AppHandle, mode: OverlayMode) -> CommandResult<()> {
    let window = app
        .get_webview_window(OVERLAY_WINDOW_LABEL)
        .ok_or_else(|| CommandError::new("OVERLAY_WINDOW_MISSING", "覆盖层窗口不存在"))?;
    let settings = settings_snapshot(app);
    let previous = current_mode();
    if let Ok(mut current) = CURRENT_MODE.lock() {
        *current = mode;
    }
    if let Some(monitor) = overlay_monitor(&window) {
        let size = match mode {
            OverlayMode::Hud => HUD_SIZE,
            OverlayMode::Panel => panel_size_on(&monitor),
        };
        window.set_size(LogicalSize::new(size.0, size.1))?;
        window.set_position(anchored_position(
            &monitor,
            Corner::parse(&settings.overlay_corner),
            size,
        ))?;
    }
    // HUD 默认点击穿透，让输入直达游戏；面板恢复交互。
    window.set_ignore_cursor_events(mode == OverlayMode::Hud && settings.overlay_click_through)?;
    if mode == OverlayMode::Panel {
        // 打开面板前记录前台窗口（通常是游戏），收起时归还键盘焦点。
        remember_foreground(&window);
        window.show()?;
        window.set_focus()?;
    } else if previous == OverlayMode::Panel {
        restore_foreground();
    }
    let _ = app.emit(OVERLAY_MODE_EVENT, mode.as_str());
    Ok(())
}

/// 面板打开前的前台窗口（焦点归还用）。None = 未记录或前台就是覆盖层自身。
static FOREGROUND_BEFORE_PANEL: Mutex<Option<isize>> = Mutex::new(None);

/// 记录当前前台窗口句柄（覆盖层自身除外），供面板收起时归还焦点。
#[cfg(windows)]
fn remember_foreground(window: &WebviewWindow) {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    let foreground = unsafe { GetForegroundWindow() } as isize;
    let overlay_hwnd = window.hwnd().map(|hwnd| hwnd.0 as isize).unwrap_or(-1);
    let captured = (foreground != 0 && foreground != overlay_hwnd).then_some(foreground);
    if let Ok(mut saved) = FOREGROUND_BEFORE_PANEL.lock() {
        *saved = captured;
    }
}

#[cfg(windows)]
fn restore_foreground() {
    use windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow;
    let saved = FOREGROUND_BEFORE_PANEL.lock().ok().and_then(|value| *value);
    if let Some(hwnd) = saved {
        unsafe {
            SetForegroundWindow(hwnd as _);
        }
    }
}

// 非 Windows 平台为空实现，焦点归还暂不处理。
#[cfg(not(windows))]
fn remember_foreground(_window: &WebviewWindow) {}

#[cfg(not(windows))]
fn restore_foreground() {}

/// 热键/命令入口：HUD ↔ 面板切换；窗口不存在时先创建并进入面板。
pub fn toggle_panel(app: &AppHandle) -> CommandResult<()> {
    ensure_overlay_window(app)?;
    let next = if current_mode() == OverlayMode::Panel {
        OverlayMode::Hud
    } else {
        OverlayMode::Panel
    };
    apply_mode(app, next)
}

/// 命令入口：切换到指定模式。
pub fn set_mode(app: &AppHandle, mode: OverlayMode) -> CommandResult<()> {
    ensure_overlay_window(app)?;
    apply_mode(app, mode)
}

/// 命令入口：显示覆盖层（保持当前模式，默认 HUD）。
pub fn show_overlay(app: &AppHandle) -> CommandResult<()> {
    let window = ensure_overlay_window(app)?;
    apply_mode(app, current_mode())?;
    window.show()?;
    Ok(())
}

/// 隐藏覆盖层（保留 WebView）。
pub fn hide_overlay(app: &AppHandle) -> CommandResult<()> {
    if let Some(window) = app.get_webview_window(OVERLAY_WINDOW_LABEL) {
        window.hide()?;
    }
    Ok(())
}

/// 销毁覆盖层窗口（停用功能时释放资源）。
fn destroy_overlay_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(OVERLAY_WINDOW_LABEL) {
        let _ = window.destroy();
    }
    if let Ok(mut mode) = CURRENT_MODE.lock() {
        *mode = OverlayMode::Hud;
    }
}

/// 任一客户端在运行即视为游戏中。
fn game_running(app: &AppHandle) -> bool {
    app.state::<AppState>()
        .game_monitor
        .current
        .lock()
        .map(|status| status.clients.iter().any(|client| client.running))
        .unwrap_or(false)
}

/// game_session 监控钩子：游戏启动 → 显示 HUD、拉起 tosu 与 IRC 监听；
/// 游戏退出 → 隐藏窗口（不销毁）、断开 IRC。
pub fn on_game_status(app: &AppHandle, running: bool) {
    let settings = settings_snapshot(app);
    if !settings.overlay_enabled {
        return;
    }
    if running {
        // 覆盖层依赖 tosu 数据：未运行且已配置时自动拉起（托管启动不弹浏览器）。
        if settings.overlay_start_tosu_with_game
            && !crate::features::tosu::process_running()
            && settings.tosu_executable_path.is_some()
        {
            let state = app.state::<AppState>();
            let _ = crate::features::tosu::start_managed_tosu(&state, app.clone());
        }
        super::irc::ensure_irc_connection(app);
        if settings.overlay_show_with_game {
            let _ = show_overlay(app);
        }
    } else {
        super::irc::stop_irc(app);
        if settings.overlay_auto_hide_on_game_exit {
            let _ = hide_overlay(app);
        }
    }
}

/// 同步全局热键：先清空再按设置注册；返回错误供设置保存时回显。
pub fn sync_hotkey(app: &AppHandle, settings: &AppSettings) -> CommandResult<()> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
    let shortcuts = app.global_shortcut();
    let _ = shortcuts.unregister_all();
    if !settings.overlay_enabled {
        return Ok(());
    }
    let shortcut: Shortcut = settings.overlay_hotkey_toggle.parse().map_err(|_| {
        CommandError::new(
            "INVALID_OVERLAY_HOTKEY",
            format!("覆盖层热键无效：{}", settings.overlay_hotkey_toggle),
        )
    })?;
    let app_handle = app.clone();
    shortcuts
        .on_shortcut(shortcut, move |_app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                let _ = toggle_panel(&app_handle);
            }
        })
        .map_err(|error| {
            CommandError::new(
                "OVERLAY_HOTKEY_FAILED",
                format!("注册覆盖层热键失败（可能与其他程序冲突）：{error}"),
            )
        })?;
    Ok(())
}

/// 设置更新钩子：热键重注册、窗口按启用状态创建/销毁、角落变化重排。
pub fn apply_settings(app: &AppHandle, settings: &AppSettings) {
    if let Err(error) = sync_hotkey(app, settings) {
        crate::log_warn!("overlay", "覆盖层热键更新失败：{}", error.message);
    }
    // tosu 游戏内渲染集成随开关安装/卸载（幂等）。
    if settings.overlay_ingame_enabled {
        let state = app.state::<AppState>();
        if let Err(error) = super::ingame::install_ingame_overlay(&state) {
            crate::log_warn!("overlay", "安装 tosu 游戏内渲染失败：{}", error.message);
        }
    } else if let Some(state_result) = app.try_state::<AppState>() {
        // 未启用时清理残留安装（tosu 未配置等情况静默跳过）。
        let _ = super::ingame::uninstall_ingame_overlay(&state_result);
    }
    if settings.overlay_enabled {
        match ensure_overlay_window(app) {
            Ok(window) => {
                // 角落/尺寸设置可能变化，按当前模式重排；HUD 模式不改变可见性，
                // Panel 模式 apply_mode 自带 show+focus。
                let _ = apply_mode(app, current_mode());
                if !window.is_visible().unwrap_or(false)
                    && game_running(app)
                    && settings.overlay_show_with_game
                {
                    let _ = show_overlay(app);
                }
                if game_running(app) {
                    super::irc::ensure_irc_connection(app);
                }
            }
            Err(error) => {
                crate::log_warn!("overlay", "创建覆盖层窗口失败：{}", error.message);
            }
        }
    } else {
        destroy_overlay_window(app);
        super::irc::stop_irc(app);
    }
}

/// 应用启动时初始化：状态服务与下载进度监听常驻；启用状态下注册热键、
/// 预建隐藏窗口并按游戏状态决定 IRC/窗口显示。
pub fn initialize(app: &AppHandle) {
    // 本地状态服务 + 下载进度监听：tosu 游戏内渲染页的数据源。
    super::ingame::ensure_status_server(app);
    listen_download_progress(app);
    let settings = settings_snapshot(app);
    if !settings.overlay_enabled {
        return;
    }
    if let Err(error) = sync_hotkey(app, &settings) {
        crate::log_warn!("overlay", "注册覆盖层热键失败：{}", error.message);
    }
    if settings.overlay_ingame_enabled {
        let state = app.state::<AppState>();
        if let Err(error) = super::ingame::install_ingame_overlay(&state) {
            crate::log_warn!("overlay", "安装 tosu 游戏内渲染失败：{}", error.message);
        }
    }
    if let Err(error) = ensure_overlay_window(app) {
        crate::log_warn!("overlay", "创建覆盖层窗口失败：{}", error.message);
    } else if game_running(app) {
        super::irc::ensure_irc_connection(app);
        if settings.overlay_show_with_game {
            let _ = show_overlay(app);
        }
    }
}

/// 监听下载进度事件并写入覆盖层运行时（HTTP 状态服务消费）。
fn listen_download_progress(app: &AppHandle) {
    use tauri::Listener;
    let overlay = app.state::<AppState>().overlay.clone();
    app.listen("beatmap-download-progress", move |event| {
        let Ok(progress) = serde_json::from_str::<
            crate::features::online_beatmaps::BeatmapDownloadProgress,
        >(event.payload()) else {
            return;
        };
        let finished = matches!(progress.phase.as_str(), "finished" | "cancelled" | "failed");
        if let Ok(mut slot) = overlay.download.lock() {
            *slot = if finished { None } else { Some(progress) };
        }
    });
}

// 模式与角落锚点解析（含未知值回退）的单测。
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_overlay_modes() {
        assert_eq!(OverlayMode::parse("hud").unwrap(), OverlayMode::Hud);
        assert_eq!(OverlayMode::parse("panel").unwrap(), OverlayMode::Panel);
        assert!(OverlayMode::parse("other").is_err());
    }

    #[test]
    fn parses_corners_with_default() {
        assert_eq!(Corner::parse("TopLeft"), Corner::TopLeft);
        assert_eq!(Corner::parse("BottomLeft"), Corner::BottomLeft);
        assert_eq!(Corner::parse("BottomRight"), Corner::BottomRight);
        assert_eq!(Corner::parse("whatever"), Corner::TopRight);
        assert_eq!(Corner::parse("TopRight"), Corner::TopRight);
    }
}
