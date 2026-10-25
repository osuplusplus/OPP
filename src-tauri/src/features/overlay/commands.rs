//! 覆盖层的 Tauri 命令：仅做参数反序列化、服务调用与错误转换。

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::{
    error::{CommandError, CommandResult},
    state::AppState,
};

use super::service::{self, OVERLAY_WINDOW_LABEL};

/// 覆盖层状态查询结果；窗口尚未创建时按 HUD 默认值返回。
#[derive(Debug, Serialize)]
pub struct OverlayStateInfo {
    pub enabled: bool,
    pub visible: bool,
    pub mode: String,
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：读取覆盖层当前状态。
pub fn overlay_get_state(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<OverlayStateInfo> {
    let enabled = state
        .store
        .settings_snapshot()
        .is_ok_and(|settings| settings.overlay_enabled);
    match app.get_webview_window(OVERLAY_WINDOW_LABEL) {
        Some(window) => Ok(OverlayStateInfo {
            enabled,
            visible: window.is_visible().unwrap_or(false),
            mode: service::current_mode_str(),
        }),
        None => Ok(OverlayStateInfo {
            enabled,
            visible: false,
            mode: "hidden".into(),
        }),
    }
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：切换 隐藏/HUD/面板 模式。
pub fn overlay_set_mode(mode: String, app: AppHandle) -> CommandResult<()> {
    let mode = service::OverlayMode::parse(&mode)?;
    service::set_mode(&app, mode)
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：收起面板（Esc/关闭按钮），回到唤出前的状态。
pub fn overlay_close_panel(app: AppHandle) -> CommandResult<()> {
    service::close_panel(&app)
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：显示或隐藏覆盖层。
pub fn overlay_set_visible(visible: bool, app: AppHandle) -> CommandResult<()> {
    if visible {
        service::show_overlay(&app)
    } else {
        service::hide_overlay(&app)
    }
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：校验热键字符串能否注册（设置页保存前预检）。
/// 强制按启用状态注册以真实校验，随后立即按已保存设置恢复，避免预检残留新热键。
pub fn overlay_test_hotkey(
    hotkey: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    let saved = state
        .store
        .settings_snapshot()
        .map_err(|_| CommandError::new("SETTINGS_LOCKED", "设置状态不可用"))?;
    let mut candidate = saved.clone();
    candidate.overlay_enabled = true;
    candidate.overlay_hotkey_toggle = hotkey;
    service::sync_hotkey(&app, &candidate)?;
    service::sync_hotkey(&app, &saved)
}

/// MP/IRC 面板聚合状态：连接与补缺活动、凭据配置情况与游戏内渲染安装状态。
#[derive(Debug, Serialize)]
pub struct OverlayMpStatus {
    pub irc: super::irc::IrcStatusInfo,
    pub mp: super::irc::MpActivityInfo,
    pub irc_configured: bool,
    pub irc_username: Option<String>,
    pub ingame_installed: bool,
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：读取 IRC/MP 补缺与游戏内渲染安装状态。
pub fn overlay_get_mp_status(state: State<'_, AppState>) -> CommandResult<OverlayMpStatus> {
    let settings = state
        .store
        .settings_snapshot()
        .map_err(|_| CommandError::new("SETTINGS_LOCKED", "设置状态不可用"))?;
    let runtime = &state.overlay;
    Ok(OverlayMpStatus {
        irc: runtime
            .irc
            .lock()
            .map(|value| value.clone())
            .unwrap_or_default(),
        mp: runtime
            .mp
            .lock()
            .map(|value| value.clone())
            .unwrap_or_default(),
        irc_configured: state
            .credentials
            .get_osu_irc_token()
            .ok()
            .flatten()
            .is_some_and(|token| !token.is_empty()),
        irc_username: settings.overlay_irc_username.clone(),
        ingame_installed: super::ingame::ingame_installed(&state),
    })
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：保存 osu! IRC 凭据（令牌入系统密钥环）。
/// 游戏运行中时立即按新凭据重连。
pub async fn overlay_set_irc_credentials(
    username: Option<String>,
    token: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    let username = username
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    if username.is_none() && !token.is_empty() {
        return Err(CommandError::new(
            "INVALID_IRC_USERNAME",
            "请填写 osu! 用户名（IRC 昵称）",
        ));
    }
    let store = state.store.clone();
    let saved = store
        .settings_snapshot()
        .map_err(|_| CommandError::new("SETTINGS_LOCKED", "设置状态不可用"))?;
    let mut next = saved.clone();
    next.overlay_irc_username = username;
    crate::infrastructure::tasks::blocking_io("update_settings", move || {
        store.update_settings(|stored| {
            stored.overlay_irc_username = next.overlay_irc_username.clone();
            stored.clone()
        })
    })
    .await??;
    let query_client = app.clone();
    state.credentials.set_osu_irc_token(token.trim())?;
    // 凭据变更后立即重连：先停旧任务（若有），再按需拉起。
    super::irc::stop_irc(&query_client);
    super::irc::ensure_irc_connection(&query_client);
    Ok(())
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：安装/更新 OPP HUD 到 tosu 游戏内渲染。
pub fn overlay_install_ingame(app: AppHandle) -> CommandResult<super::ingame::IngameInstallInfo> {
    let state = app.state::<AppState>();
    super::ingame::install_ingame_overlay(&state)
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：从 tosu 游戏内渲染卸载 OPP HUD。
pub fn overlay_uninstall_ingame(app: AppHandle) -> CommandResult<()> {
    let state = app.state::<AppState>();
    super::ingame::uninstall_ingame_overlay(&state)
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：读取 MP 频道聊天缓冲（时间正序）。
pub fn overlay_get_chat(
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> CommandResult<Vec<super::irc::ChatEntry>> {
    let runtime = &state.overlay;
    let log = runtime
        .chat
        .lock()
        .map_err(|_| CommandError::new("CHAT_BUFFER_LOCKED", "聊天缓冲区暂不可用"))?;
    Ok(log.recent(limit.unwrap_or(100).min(super::irc::CHAT_HISTORY_LIMIT)))
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：把文本写入系统剪贴板（聊天译文复制粘贴用）。
pub fn overlay_copy_text(text: String) -> CommandResult<()> {
    // 打开剪贴板是快操作，直接在命令线程完成。
    let mut clipboard = arboard::Clipboard::new().map_err(|error| {
        CommandError::new(
            "CLIPBOARD_UNAVAILABLE",
            format!("系统剪贴板不可用：{error}"),
        )
    })?;
    clipboard.set_text(text).map_err(|error| {
        CommandError::new("CLIPBOARD_WRITE_FAILED", format!("写入剪贴板失败：{error}"))
    })
}
