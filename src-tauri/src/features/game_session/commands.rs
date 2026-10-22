//! Tauri 命令层：从 mod.rs 拆分，命令名与序列化契约不变。

use super::*;
use crate::error::{CommandError, CommandResult};
use crate::state::AppState;
use tauri::{AppHandle, State};

/// 返回监视循环缓存的最新客户端状态快照（不触发即时进程扫描）。
#[tauri::command]
/// 供前端调用的 Tauri 命令：读取当前状态或详情。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn get_game_status(state: State<'_, AppState>) -> CommandResult<GameStatusSnapshot> {
    state
        .game_monitor
        .current
        .lock()
        .map(|current| current.clone())
        .map_err(|_| CommandError::new("GAME_STATUS_LOCKED", "游戏状态不可用"))
}

/// 逐个把 `.osz` 曲包路径作为启动参数交给客户端打开，两次启动间隔 180ms；
/// 不存在、非 `.osz` 或启动失败者计入失败列表。
#[tauri::command]
pub async fn open_beatmap_files(
    client: LocalClient,
    archive_paths: Vec<String>,
    state: State<'_, AppState>,
) -> CommandResult<BeatmapOpenResult> {
    use crate::infrastructure::logging::{finish_span, global};
    let span = global().map(|log| log.operation("game_session", "open_beatmap_files"));
    let result = async {
        let target = game_launch_target(client, &state)?;
        let mut opened = 0usize;
        let mut failures = Vec::new();
        let total = archive_paths.len();
        for (index, value) in archive_paths.into_iter().enumerate() {
            let path = PathBuf::from(&value);
            let valid = path.is_file()
                && path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("osz"));
            if !valid {
                failures.push(format!("曲包文件不存在或格式不受支持：{value}"));
                continue;
            }
            let mut launch = Command::new(&target.exe);
            launch.arg(&path);
            if let Some(dir) = &target.working_dir {
                launch.current_dir(dir);
            }
            #[cfg(windows)]
            launch.creation_flags(CREATE_NO_WINDOW);
            let launched = launch.spawn();
            if let Some(span) = &span {
                span.io("launch_beatmap_file", &launched);
            }
            match launched {
                Ok(_) => opened += 1,
                Err(error) => failures.push(format!(
                    "无法使用 {client} 打开 {}：{error}",
                    path.display()
                )),
            }
            if index + 1 < total {
                tokio::time::sleep(Duration::from_millis(180)).await;
            }
        }
        Ok(BeatmapOpenResult {
            opened,
            failed: failures.len(),
            failures,
        })
    }
    .await;
    finish_span(span, result)
}

/// 把 `osu://b/<id>` 作为启动参数直接传给 lazer，不依赖系统 `osu://` 协议注册。
/// Pass the official URI directly to lazer, independently of the default osu:// handler.
#[tauri::command]
pub fn open_lazer_beatmap(beatmap_id: i32, state: State<'_, AppState>) -> CommandResult<()> {
    use crate::infrastructure::logging::{finish_span, global};
    let span = global().map(|log| log.operation("game_session", "open_lazer_beatmap"));
    let result = (|| {
        let uri = lazer_beatmap_uri(beatmap_id)?;
        let target = game_launch_target(LocalClient::Lazer, &state)?;
        let mut launch = Command::new(&target.exe);
        launch.arg(uri);
        if let Some(dir) = target.working_dir {
            launch.current_dir(dir);
        }
        #[cfg(windows)]
        launch.creation_flags(CREATE_NO_WINDOW);
        let launched = launch.spawn().map_err(|e| {
            CommandError::new(
                "GAME_START_FAILED",
                format!("无法打开 lazer，请检查安装路径：{e}"),
            )
        });
        if let Some(span) = &span {
            span.io("launch_lazer", &launched);
        }
        launched.map(|_| ())
    })();
    finish_span(span, result)
}

/// 启动客户端并开启游玩会话：可选先拉起 tosu，并按设置联动 OTD；记录起始
/// 资料快照。客户端已在运行时直接报错。
#[tauri::command]
/// 供前端调用的 Tauri 命令：启动后台任务或外部服务。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn start_game_session(
    ruleset: Ruleset,
    client: LocalClient,
    launch_tosu: Option<bool>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> CommandResult<GameSessionSummary> {
    let target = game_launch_target(client, &state)?;
    if client_running(client, &target.exe) {
        return Err(CommandError::new("GAME_ALREADY_RUNNING", "osu! 已经在运行"));
    }
    if launch_tosu.unwrap_or(false) {
        start_managed_tosu(&state, app)?;
    }
    if state.store.settings_snapshot()?.launch_otd_with_game {
        start_managed_otd(&state)?;
    }
    let start = snapshot(&state, ruleset).await?;
    let mut launch = Command::new(&target.exe);
    if let Some(dir) = &target.working_dir {
        launch.current_dir(dir);
    }
    #[cfg(windows)]
    launch.creation_flags(CREATE_NO_WINDOW);
    launch
        .spawn()
        .map_err(|e| CommandError::new("GAME_START_FAILED", format!("无法启动 osu!：{e}")))?;
    let summary = GameSessionSummary {
        started_at: Utc::now(),
        ended_at: None,
        ruleset,
        client: client.to_string(),
        executable: target.exe.display().to_string(),
        start,
        end: None,
        running: true,
    };
    *state
        .game_session
        .active
        .lock()
        .map_err(|_| CommandError::new("SESSION_LOCKED", "游戏会话状态不可用"))? =
        Some(summary.clone());
    Ok(summary)
}

/// 为 OPP 之外启动的游戏补建会话：须检测到客户端在运行；同一可执行文件的
/// 会话已在进行则直接返回，不重复抓取快照。
/// Starts a comparable session for an osu! process launched outside OPP.
/// The process monitor calls this after a running client is observed, so the
/// normal end-of-session poll can still produce a before/after summary.
#[tauri::command]
/// 供前端调用的 Tauri 命令：启动后台任务或外部服务。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn start_detected_game_session(
    ruleset: Ruleset,
    client: LocalClient,
    state: State<'_, AppState>,
) -> CommandResult<GameSessionSummary> {
    let exe = game_launch_target(client, &state)?.exe;
    if !client_running(client, &exe) {
        return Err(CommandError::new(
            "GAME_NOT_RUNNING",
            "未检测到正在运行的 osu! 客户端",
        ));
    }
    {
        let active = state
            .game_session
            .active
            .lock()
            .map_err(|_| CommandError::new("SESSION_LOCKED", "游戏会话状态不可用"))?;
        if let Some(summary) = active.as_ref()
            && summary.running
            && same_executable(Path::new(&summary.executable), &exe)
        {
            return Ok(summary.clone());
        }
    }
    let start = snapshot(&state, ruleset).await?;
    let summary = GameSessionSummary {
        started_at: Utc::now(),
        ended_at: None,
        ruleset,
        client: client.to_string(),
        executable: exe.display().to_string(),
        start,
        end: None,
        running: true,
    };
    *state
        .game_session
        .active
        .lock()
        .map_err(|_| CommandError::new("SESSION_LOCKED", "游戏会话状态不可用"))? =
        Some(summary.clone());
    Ok(summary)
}

/// 轮询会话状态：标记仍在运行的会话会复查进程，发现已退出时补写结束时间
/// 与终点资料快照后写回。
#[tauri::command]
/// 供前端调用的 Tauri 命令：读取当前状态或详情。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn get_game_session_status(
    state: State<'_, AppState>,
) -> CommandResult<Option<GameSessionSummary>> {
    let current = state
        .game_session
        .active
        .lock()
        .map_err(|_| CommandError::new("SESSION_LOCKED", "游戏会话状态不可用"))?
        .clone();
    let Some(mut summary) = current else {
        return Ok(None);
    };
    if summary.running {
        let still_running = client_from_str(&summary.client)
            .is_some_and(|client| client_running(client, Path::new(&summary.executable)));
        if !still_running {
            summary.running = false;
            summary.ended_at = Some(Utc::now());
            summary.end = Some(snapshot(&state, summary.ruleset).await?);
            *state
                .game_session
                .active
                .lock()
                .map_err(|_| CommandError::new("SESSION_LOCKED", "游戏会话状态不可用"))? =
                Some(summary.clone());
        }
    }
    Ok(Some(summary))
}

/// 在系统资源管理器中定位媒体文件；扩展名与目录门禁与读取命令一致。
#[tauri::command]
/// 供前端调用的 Tauri 命令：在系统中打开资源或输出位置。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn open_media_in_explorer(
    client: LocalClient,
    path: String,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    let candidate = PathBuf::from(&path)
        .canonicalize()
        .map_err(|e| CommandError::new("MEDIA_NOT_FOUND", e.to_string()))?;
    let extension = candidate
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let allowed_extension =
        extension == "osr" || ["png", "jpg", "jpeg", "webp"].contains(&extension.as_str());
    let allowed = candidate.is_file()
        && allowed_extension
        && media_roots(&state, client)?
            .into_iter()
            .any(|root| within_root(&candidate, &root));
    if !allowed {
        return Err(CommandError::new(
            "MEDIA_PATH_NOT_ALLOWED",
            "媒体文件不在 osu! 数据目录内",
        ));
    }

    crate::infrastructure::platform::reveal_path(&candidate)
        .map_err(|error| CommandError::new("EXPLORER_OPEN_FAILED", error.to_string()))?;
    Ok(())
}
