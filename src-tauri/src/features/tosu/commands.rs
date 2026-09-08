use std::path::PathBuf;

use tauri::{AppHandle, Manager, State};

use crate::{
    domain::AppSettings,
    error::{CommandError, CommandResult},
    state::AppState,
};

use super::{
    models::{self, TosuLogEntry, TosuStatus},
    service,
};

fn settings(state: &AppState) -> CommandResult<AppSettings> {
    Ok(state.store.snapshot()?.settings)
}

async fn api_reachable(base: &str) -> bool {
    let Ok(base) = service::normalize_base_url(base) else {
        return false;
    };
    reqwest::Client::new()
        .get(format!("{base}/json/v2"))
        .send()
        .await
        .is_ok()
}

fn current_status(state: &AppState, settings: &AppSettings, api_reachable: bool) -> TosuStatus {
    let configured_valid = settings
        .tosu_executable_path
        .as_ref()
        .is_some_and(|path| service::validate_executable(PathBuf::from(path).as_path()).is_ok());
    // Linux：PATH 中能找到 tosu 即视为已安装（启动时经 pkexec/sudo 提权），
    // 显示时也回退到 PATH 解析出的绝对路径。
    #[cfg(not(windows))]
    let (installed, executable_path) = {
        let from_path = crate::infrastructure::platform::find_in_path("tosu")
            .map(|path| path.display().to_string());
        (
            configured_valid || from_path.is_some(),
            settings.tosu_executable_path.clone().or(from_path),
        )
    };
    #[cfg(windows)]
    let (installed, executable_path) = (configured_valid, settings.tosu_executable_path.clone());
    let owned_by_opp = service::is_owned_running(&state.tosu);
    let lyrics_configured_valid =
        settings
            .tosu_lyrics_executable_path
            .as_ref()
            .is_some_and(|path| {
                service::validate_lyrics_executable(PathBuf::from(path).as_path()).is_ok()
            });
    // Linux：PATH 中能找到 tosu-proxy 即视为已安装，显示时也回退到 PATH 解析出的
    // 绝对路径（与 tosu 自身一致）。
    #[cfg(not(windows))]
    let (lyrics_installed, lyrics_executable_path) = {
        let from_path = crate::infrastructure::platform::find_in_path("tosu-proxy")
            .map(|path| path.display().to_string());
        (
            lyrics_configured_valid || from_path.is_some(),
            settings.tosu_lyrics_executable_path.clone().or(from_path),
        )
    };
    #[cfg(windows)]
    let (lyrics_installed, lyrics_executable_path) = (
        lyrics_configured_valid,
        settings.tosu_lyrics_executable_path.clone(),
    );
    TosuStatus {
        installed,
        executable_path,
        api_base_url: settings.tosu_api_base_url.clone(),
        api_reachable,
        running: owned_by_opp || service::process_running() || api_reachable,
        owned_by_opp,
        dashboard_url: settings.tosu_api_base_url.clone(),
        last_error: service::last_error(&state.tosu),
        lyrics: models::TosuLyricsStatus {
            installed: lyrics_installed,
            executable_path: lyrics_executable_path,
            running: service::is_lyrics_owned_running(&state.tosu)
                || service::lyrics_process_running(),
            owned_by_opp: service::is_lyrics_owned_running(&state.tosu),
            proxy_url: "http://127.0.0.1:41280/lyrics/".into(),
        },
    }
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：读取当前状态或详情。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn get_tosu_status(
    state: State<'_, AppState>,
    app: AppHandle,
) -> CommandResult<TosuStatus> {
    let settings = settings(&state)?;
    let reachable = api_reachable(&settings.tosu_api_base_url).await;
    if reachable {
        service::ensure_live_connection(
            state.tosu.clone(),
            settings.tosu_api_base_url.clone(),
            app,
        );
    }
    Ok(current_status(&state, &settings, reachable))
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：读取当前状态或详情。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn get_tosu_logs(state: State<'_, AppState>) -> CommandResult<Vec<TosuLogEntry>> {
    Ok(service::logs(&state.tosu))
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：更新运行时或持久化配置。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn set_tosu_executable(path: String, state: State<'_, AppState>) -> CommandResult<TosuStatus> {
    let executable = service::validate_executable(PathBuf::from(path).as_path())?;
    let saved = executable.display().to_string();
    state
        .store
        .update(|persisted| persisted.settings.tosu_executable_path = Some(saved))?;
    let settings = settings(&state)?;
    Ok(current_status(&state, &settings, false))
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：更新运行时或持久化配置。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn set_tosu_lyrics_executable(
    path: String,
    state: State<'_, AppState>,
) -> CommandResult<TosuStatus> {
    let executable = service::validate_lyrics_executable(PathBuf::from(path).as_path())?;
    let saved = executable.display().to_string();
    state
        .store
        .update(|persisted| persisted.settings.tosu_lyrics_executable_path = Some(saved))?;
    let settings = settings(&state)?;
    Ok(current_status(&state, &settings, false))
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：启动后台任务或外部服务。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn start_tosu(state: State<'_, AppState>, app: AppHandle) -> CommandResult<()> {
    start_managed_tosu(&state, app)
}

pub fn start_managed_tosu(state: &AppState, app: AppHandle) -> CommandResult<()> {
    let settings = settings(state)?;
    service::start(state.tosu.clone(), &settings, app.clone())?;
    let runtime = state.tosu.clone();
    let base = settings.tosu_api_base_url;
    tauri::async_runtime::spawn(async move {
        for _ in 0..30 {
            if api_reachable(&base).await {
                match crate::features::obs::refresh_selected(&app.state::<AppState>()).await {
                    Ok(result) => service::log_system(&runtime, &app, result.message),
                    Err(error) => service::log_system(
                        &runtime,
                        &app,
                        format!("OBS 浏览器源刷新失败：{}", error.message),
                    ),
                }
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        service::log_system(
            &runtime,
            &app,
            "tosu 未在 15 秒内就绪，已跳过 OBS 浏览器源刷新",
        );
    });
    Ok(())
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：停止受应用管理的服务。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn stop_tosu(state: State<'_, AppState>, app: AppHandle) -> CommandResult<()> {
    // pkexec 认证会阻塞到用户在弹窗输入密码，放进 blocking 线程避免卡住 UI。
    let runtime = state.tosu.clone();
    tauri::async_runtime::spawn_blocking(move || service::stop(&runtime, &app))
        .await
        .map_err(|_| CommandError::new("TOSU_STOP_FAILED", "终止 tosu 的任务异常退出"))?
}
