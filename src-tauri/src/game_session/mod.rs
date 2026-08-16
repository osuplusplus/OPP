mod models;

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    time::{Duration, UNIX_EPOCH},
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::Utc;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{
    account::ensure_access_token,
    error::{CommandError, CommandResult},
    local_analysis::LocalClient,
    models::Ruleset,
    state::AppState,
    tosu::start_managed_tosu,
};

use models::{
    GameClientStatus, GameMediaItem, GameReplayPayload, GameScreenshotPayload, GameSessionSummary,
    GameStatusSnapshot, NewReplayItem, NewReplaysDetected, ReplayFingerprint, ReplayMapInfo,
    ReplayWatchSession, UserSnapshot,
};
pub use models::{GameMonitorRuntime, GameSessionRuntime};

fn number(value: &serde_json::Value, key: &str) -> Option<u64> {
    value
        .get(key)
        .and_then(|v| v.as_u64().or_else(|| v.as_f64().map(|n| n as u64)))
}

fn decimal(value: &serde_json::Value, key: &str) -> Option<f64> {
    value
        .get(key)
        .and_then(|v| v.as_f64().or_else(|| v.as_u64().map(|n| n as f64)))
}

async fn snapshot(state: &AppState, ruleset: Ruleset) -> CommandResult<UserSnapshot> {
    let token = ensure_access_token(state).await?;
    let profile = state.api.get_own_profile(&token, ruleset).await?;
    let stats = profile
        .statistics
        .as_ref()
        .unwrap_or(&serde_json::Value::Null);
    Ok(UserSnapshot {
        captured_at: Utc::now(),
        username: profile.username,
        pp: decimal(stats, "pp"),
        ranked_score: number(stats, "ranked_score"),
        hit_accuracy: decimal(stats, "hit_accuracy"),
        total_hits: number(stats, "total_hits"),
        total_score: number(stats, "total_score"),
    })
}

#[cfg(windows)]
fn running_executables() -> Vec<PathBuf> {
    Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-Process -Name 'osu!' -ErrorAction SilentlyContinue | ForEach-Object { $_.Path }",
        ])
        .output()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(str::trim)
                .filter(|path| !path.is_empty())
                .map(PathBuf::from)
                .collect()
        })
        .unwrap_or_default()
}

fn same_executable(left: &Path, right: &Path) -> bool {
    left.to_string_lossy()
        .eq_ignore_ascii_case(&right.to_string_lossy())
}

#[cfg(windows)]
fn executable_running(executable: &Path, running: &[PathBuf]) -> bool {
    running.iter().any(|path| same_executable(path, executable))
}

/// 判断客户端当前是否正在运行。Windows 按可执行文件路径比对进程列表；Linux
/// 下游戏经 wine / pressure-vessel 等包装层启动，实际进程名与启动命令不同
/// （stable 表现为 `D:\osu!.exe`），改按 `/proc` 匹配真实进程。
fn client_running(_client: LocalClient, executable: &Path) -> bool {
    #[cfg(windows)]
    {
        executable_running(executable, &running_executables())
    }
    #[cfg(not(windows))]
    {
        let _ = executable;
        crate::platform::game_process_running(&_client.to_string())
    }
}

fn client_from_str(value: &str) -> Option<LocalClient> {
    match value {
        "stable" => Some(LocalClient::Stable),
        "lazer" => Some(LocalClient::Lazer),
        _ => None,
    }
}

pub(crate) fn executable(client: LocalClient, root: &str) -> Option<PathBuf> {
    #[cfg(not(windows))]
    {
        let _ = root;
        crate::platform::game_command(&client.to_string()).map(PathBuf::from)
    }
    #[cfg(windows)]
    {
        let root = Path::new(root);
        let names = if client == LocalClient::Lazer {
            vec![root.join("current").join("osu!.exe"), root.join("osu!.exe")]
        } else {
            vec![root.join("osu!.exe")]
        };
        names.into_iter().find(|path| path.is_file())
    }
}

fn scan_game_status(
    local_analysis: &crate::local_analysis::LocalAnalysisService,
) -> GameStatusSnapshot {
    #[cfg(not(windows))]
    let _ = local_analysis;
    let detected_at = Utc::now();
    let clients = [LocalClient::Stable, LocalClient::Lazer]
        .into_iter()
        .map(|client| {
            let executable = {
                #[cfg(windows)]
                {
                    local_analysis
                        .source_status(client)
                        .ok()
                        .and_then(|source| source.install_root)
                        .and_then(|root| executable(client, &root))
                }
                #[cfg(not(windows))]
                {
                    executable(client, "")
                }
            };
            GameClientStatus {
                client,
                running: executable
                    .as_deref()
                    .is_some_and(|path| client_running(client, path)),
                executable: executable.map(|path| path.display().to_string()),
                detected_at,
            }
        })
        .collect();
    GameStatusSnapshot { clients }
}

fn any_client_started(previous: &GameStatusSnapshot, next: &GameStatusSnapshot) -> bool {
    next.clients.iter().any(|after| {
        after.running
            && !previous
                .clients
                .iter()
                .any(|before| before.client == after.client && before.running)
    })
}

fn replay_snapshot(
    state: &AppState,
    client: LocalClient,
) -> CommandResult<HashMap<String, ReplayFingerprint>> {
    let mut snapshot = HashMap::new();
    for root in media_roots(state, client)? {
        for entry in walkdir::WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            let path = entry.path();
            if !path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("osr"))
            {
                continue;
            }
            let Ok(path) = path.canonicalize() else {
                continue;
            };
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            let modified_at_millis = metadata
                .modified()
                .ok()
                .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                .map(|value| value.as_millis())
                .unwrap_or_default();
            snapshot.insert(
                path.to_string_lossy().to_ascii_lowercase(),
                ReplayFingerprint {
                    path: path.display().to_string(),
                    size: metadata.len(),
                    modified_at_millis,
                },
            );
        }
    }
    Ok(snapshot)
}

fn describe_new_replay(state: &AppState, client: LocalClient, path: &str) -> NewReplayItem {
    let file_name = Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(path)
        .to_string();
    let result = (|| -> CommandResult<(Option<String>, Option<String>)> {
        let bytes = load_game_replay_file(client, path, state)?;
        if bytes.first().copied() != Some(0) {
            return Err(CommandError::new(
                "DANSER_RULESET_UNSUPPORTED",
                "Danser 仅支持 osu!standard 回放",
            ));
        }
        let (hash, username) = parse_replay_metadata(&bytes)?;
        let beatmap = state
            .local_analysis
            .find_beatmap_by_md5(client, &hash)?
            .ok_or_else(|| {
                CommandError::new(
                    "REPLAY_BEATMAP_NOT_INDEXED",
                    "未在本地谱面索引中找到对应谱面",
                )
            })?;
        Ok((
            Some(format!(
                "{} — {} [{}]",
                beatmap.artist_unicode, beatmap.title_unicode, beatmap.difficulty_name
            )),
            Some(username),
        ))
    })();
    match result {
        Ok((beatmap_title, username)) => NewReplayItem {
            path: path.into(),
            file_name,
            beatmap_title,
            username,
            renderable: true,
            reason: None,
        },
        Err(error) => NewReplayItem {
            path: path.into(),
            file_name,
            beatmap_title: None,
            username: None,
            renderable: false,
            reason: Some(error.message),
        },
    }
}

fn changed_replay_paths(
    before: &HashMap<String, ReplayFingerprint>,
    current: &HashMap<String, ReplayFingerprint>,
) -> Vec<String> {
    let mut paths: Vec<String> = current
        .iter()
        .filter(|(path, fingerprint)| before.get(*path) != Some(*fingerprint))
        .map(|(_, fingerprint)| fingerprint.path.clone())
        .collect();
    paths.sort();
    paths
}

fn handle_replay_transition(
    previous: &GameStatusSnapshot,
    next: &GameStatusSnapshot,
    monitor: &GameMonitorRuntime,
    state: &AppState,
    app: &AppHandle,
) {
    for after in &next.clients {
        let was_running = previous
            .clients
            .iter()
            .find(|before| before.client == after.client)
            .is_some_and(|before| before.running);
        if after.running && !was_running {
            if let Ok(before) = replay_snapshot(state, after.client)
                && let Ok(mut sessions) = monitor.replay_sessions.lock()
            {
                sessions.insert(
                    after.client,
                    ReplayWatchSession {
                        started_at: Utc::now(),
                        before,
                    },
                );
            }
        } else if !after.running && was_running {
            let session = monitor
                .replay_sessions
                .lock()
                .ok()
                .and_then(|mut sessions| sessions.remove(&after.client));
            let Some(session) = session else {
                continue;
            };
            let Ok(current) = replay_snapshot(state, after.client) else {
                continue;
            };
            let paths = changed_replay_paths(&session.before, &current);
            let replays: Vec<NewReplayItem> = paths
                .iter()
                .map(|path| describe_new_replay(state, after.client, path))
                .collect();
            if !replays.is_empty() {
                let _ = app.emit(
                    "new-replays-detected",
                    NewReplaysDetected {
                        client: after.client,
                        started_at: session.started_at,
                        detected_at: Utc::now(),
                        replays,
                    },
                );
            }
        }
    }
}

/// Launch the app-lifetime monitor. The event is emitted only when a client
/// changes state; the command always returns the most recent snapshot.
pub fn start_game_monitor(
    local_analysis: Arc<crate::local_analysis::LocalAnalysisService>,
    monitor: Arc<GameMonitorRuntime>,
    app: AppHandle,
) {
    tauri::async_runtime::spawn(async move {
        loop {
            let service = local_analysis.clone();
            let next = tokio::task::spawn_blocking(move || scan_game_status(&service))
                .await
                .unwrap_or_else(|_| GameStatusSnapshot {
                    clients: Vec::new(),
                });
            let (changed, game_started, previous) = monitor
                .current
                .lock()
                .map(|mut current| {
                    let previous = current.clone();
                    let changed = current.clients.len() != next.clients.len()
                        || current
                            .clients
                            .iter()
                            .zip(&next.clients)
                            .any(|(before, after)| {
                                before.client != after.client
                                    || before.running != after.running
                                    || before.executable != after.executable
                            });
                    let game_started = any_client_started(&current, &next);
                    *current = next.clone();
                    (changed, game_started, previous)
                })
                .unwrap_or((
                    false,
                    false,
                    GameStatusSnapshot {
                        clients: Vec::new(),
                    },
                ));
            if changed {
                let state = app.state::<AppState>();
                handle_replay_transition(&previous, &next, &monitor, &state, &app);
                if game_started
                    && app
                        .state::<AppState>()
                        .store
                        .snapshot()
                        .map(|saved| saved.settings.launch_tosu_on_game_detect)
                        .unwrap_or(false)
                {
                    let state = app.state::<AppState>();
                    let _ = start_managed_tosu(&state, app.clone());
                }
                let _ = app.emit("game-status-changed", next);
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
}

#[tauri::command]
pub fn get_game_status(state: State<'_, AppState>) -> CommandResult<GameStatusSnapshot> {
    state
        .game_monitor
        .current
        .lock()
        .map(|current| current.clone())
        .map_err(|_| CommandError::new("GAME_STATUS_LOCKED", "游戏状态不可用"))
}

struct LaunchTarget {
    exe: PathBuf,
    working_dir: Option<PathBuf>,
}

/// 解析要启动/识别的客户端可执行文件。Windows 用安装目录内的 exe（并记录工作目录）；
/// Linux 用系统命令名（`osu-wine` / `osu-lazer`），无需安装目录。
fn game_launch_target(client: LocalClient, state: &AppState) -> CommandResult<LaunchTarget> {
    #[cfg(windows)]
    {
        let source = state.local_analysis.source_status(client)?;
        let root = source.install_root.ok_or_else(|| {
            CommandError::new("GAME_NOT_FOUND", format!("未找到 osu! {client} 安装目录"))
        })?;
        let exe = executable(client, &root).ok_or_else(|| {
            CommandError::new("GAME_NOT_FOUND", "安装目录中未找到 osu! 可执行文件")
        })?;
        Ok(LaunchTarget {
            exe,
            working_dir: Some(PathBuf::from(root)),
        })
    }
    #[cfg(not(windows))]
    {
        let _ = state;
        let exe = executable(client, "").ok_or_else(|| {
            CommandError::new("GAME_NOT_FOUND", format!("未找到 osu! {client} 启动命令"))
        })?;
        Ok(LaunchTarget {
            exe,
            working_dir: None,
        })
    }
}

#[tauri::command]
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

/// Starts a comparable session for an osu! process launched outside OPP.
/// The process monitor calls this after a running client is observed, so the
/// normal end-of-session poll can still produce a before/after summary.
#[tauri::command]
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

#[tauri::command]
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

pub(crate) fn media_roots(state: &AppState, client: LocalClient) -> CommandResult<Vec<PathBuf>> {
    let source = state.local_analysis.source_status(client)?;
    let mut roots = Vec::new();
    let mut add = |path: PathBuf| {
        if let Ok(path) = path.canonicalize()
            && path.is_dir()
            && !roots.iter().any(|item: &PathBuf| item == &path)
        {
            roots.push(path);
        }
    };
    let add_base = |base: PathBuf, add: &mut dyn FnMut(PathBuf)| {
        for name in ["Screenshots", "screenshots", "Replays", "replays"] {
            add(base.join(name));
        }
        let files = base.join("files");
        for name in ["Screenshots", "screenshots", "Replays", "replays"] {
            add(files.join(name));
        }
    };
    for root in source
        .install_root
        .into_iter()
        .chain(source.data_root.into_iter())
    {
        add_base(PathBuf::from(root), &mut add);
    }
    let app_names = if client == LocalClient::Stable {
        vec!["osu!"]
    } else {
        vec!["osu", "osu!"]
    };
    for env_name in ["APPDATA", "LOCALAPPDATA"] {
        if let Some(app_data) = std::env::var_os(env_name) {
            for name in &app_names {
                add_base(PathBuf::from(&app_data).join(name), &mut add);
            }
        }
    }
    Ok(roots)
}

pub(crate) fn within_root(candidate: &Path, root: &Path) -> bool {
    let candidate = candidate.to_string_lossy().to_ascii_lowercase();
    let root = root.to_string_lossy().to_ascii_lowercase();
    candidate == root
        || candidate.starts_with(&(root.clone() + "\\"))
        || candidate.starts_with(&(root + "/"))
}

pub(crate) fn load_game_replay_file(
    client: LocalClient,
    path: &str,
    state: &AppState,
) -> CommandResult<Vec<u8>> {
    let candidate = PathBuf::from(path)
        .canonicalize()
        .map_err(|error| CommandError::new("REPLAY_NOT_FOUND", error.to_string()))?;
    let allowed = media_roots(state, client)?.into_iter().any(|root| {
        within_root(&candidate, &root)
            && candidate
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("osr"))
    });
    if !allowed {
        return Err(CommandError::new(
            "REPLAY_PATH_NOT_ALLOWED",
            "回放文件不在 osu! 的 Replays 目录中",
        ));
    }
    let metadata = fs::metadata(&candidate)
        .map_err(|error| CommandError::new("REPLAY_READ_FAILED", error.to_string()))?;
    if metadata.len() > 32 * 1024 * 1024 {
        return Err(CommandError::new(
            "REPLAY_TOO_LARGE",
            "回放文件超过 32 MB，已拒绝上传",
        ));
    }
    fs::read(&candidate).map_err(|error| CommandError::new("REPLAY_READ_FAILED", error.to_string()))
}

pub(crate) fn parse_replay_metadata(bytes: &[u8]) -> CommandResult<(String, String)> {
    fn read_string(bytes: &[u8], offset: &mut usize) -> CommandResult<String> {
        let marker = *bytes
            .get(*offset)
            .ok_or_else(|| CommandError::new("REPLAY_PARSE_FAILED", "回放文件结构不完整"))?;
        *offset += 1;
        if marker == 0 {
            return Ok(String::new());
        }
        if marker != 0x0b {
            return Err(CommandError::new(
                "REPLAY_PARSE_FAILED",
                "回放文件字符串标记无效",
            ));
        }
        let mut length = 0usize;
        let mut shift = 0;
        loop {
            let byte = *bytes.get(*offset).ok_or_else(|| {
                CommandError::new("REPLAY_PARSE_FAILED", "回放文件长度字段不完整")
            })?;
            *offset += 1;
            length |= usize::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                break;
            }
            shift += 7;
            if shift > usize::BITS - 7 {
                return Err(CommandError::new(
                    "REPLAY_PARSE_FAILED",
                    "回放文件字符串过长",
                ));
            }
        }
        let end = offset
            .checked_add(length)
            .ok_or_else(|| CommandError::new("REPLAY_PARSE_FAILED", "回放文件长度溢出"))?;
        let value = std::str::from_utf8(
            bytes
                .get(*offset..end)
                .ok_or_else(|| CommandError::new("REPLAY_PARSE_FAILED", "回放文件字符串越界"))?,
        )
        .map_err(|_| CommandError::new("REPLAY_PARSE_FAILED", "回放文件字符串编码无效"))?
        .to_string();
        *offset = end;
        Ok(value)
    }
    if bytes.len() < 5 {
        return Err(CommandError::new("REPLAY_PARSE_FAILED", "回放文件过短"));
    }
    let mut offset = 5;
    let beatmap_hash = read_string(bytes, &mut offset)?;
    let username = read_string(bytes, &mut offset)?;
    Ok((beatmap_hash, username))
}

// Media libraries can contain years of replays and screenshots. Keep all
// filesystem traversal and decoding away from the window event loop.
#[tauri::command(async)]
pub fn list_game_media(
    client: LocalClient,
    state: State<'_, AppState>,
) -> CommandResult<Vec<GameMediaItem>> {
    let mut items = Vec::new();
    for root in media_roots(&state, client)? {
        for entry in walkdir::WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|x| x.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let kind = if ext == "osr" {
                "replay"
            } else if ["png", "jpg", "jpeg", "webp"].contains(&ext.as_str()) {
                "screenshot"
            } else {
                continue;
            };
            let metadata = entry
                .metadata()
                .map_err(|e| CommandError::new("MEDIA_READ_FAILED", e.to_string()))?;
            items.push(GameMediaItem {
                client,
                path: path.display().to_string(),
                kind: kind.into(),
                modified_at: metadata
                    .modified()
                    .ok()
                    .map(chrono::DateTime::<Utc>::from)
                    .map(|d| d.to_rfc3339()),
                size: metadata.len(),
            });
        }
    }
    items.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
    items.truncate(200);
    Ok(items)
}

#[tauri::command(async)]
pub fn read_game_replay(
    client: LocalClient,
    path: String,
    state: State<'_, AppState>,
) -> CommandResult<GameReplayPayload> {
    let candidate = PathBuf::from(&path)
        .canonicalize()
        .map_err(|e| CommandError::new("REPLAY_NOT_FOUND", e.to_string()))?;
    let allowed = media_roots(&state, client)?.into_iter().any(|root| {
        within_root(&candidate, &root)
            && candidate
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| x.eq_ignore_ascii_case("osr"))
    });
    if !allowed {
        return Err(CommandError::new(
            "REPLAY_PATH_NOT_ALLOWED",
            "回放文件不在 osu! 数据目录内",
        ));
    }
    let bytes = load_game_replay_file(client, &path, &state)?;
    Ok(GameReplayPayload {
        path: candidate.display().to_string(),
        file_name: candidate
            .file_name()
            .and_then(|x| x.to_str())
            .unwrap_or("replay.osr")
            .into(),
        bytes_base64: STANDARD.encode(bytes),
        video_ready: false,
        note: "已读取原始 .osr 数据；可在“回放渲染”页面提交给 o!rdr 生成视频。".into(),
    })
}

#[tauri::command(async)]
pub fn inspect_game_replay(
    client: LocalClient,
    path: String,
    state: State<'_, AppState>,
) -> CommandResult<ReplayMapInfo> {
    let bytes = load_game_replay_file(client, &path, &state)?;
    let (beatmap_hash, username) = parse_replay_metadata(&bytes)?;
    let beatmap = state
        .local_analysis
        .find_beatmap_by_md5(client, &beatmap_hash)?;
    Ok(ReplayMapInfo {
        path,
        beatmap_hash: beatmap_hash.clone(),
        username,
        beatmap_id: beatmap.as_ref().and_then(|map| map.beatmap_id),
        beatmap_resource_id: beatmap.as_ref().map(|map| map.resource.resource_id.clone()),
        beatmap_title: beatmap.as_ref().map(|map| {
            format!(
                "{} — {} [{}]",
                map.artist_unicode, map.title_unicode, map.difficulty_name
            )
        }),
        submitted: beatmap.as_ref().and_then(|map| map.beatmap_id).is_some(),
    })
}

#[tauri::command(async)]
pub fn read_game_screenshot(
    client: LocalClient,
    path: String,
    state: State<'_, AppState>,
) -> CommandResult<GameScreenshotPayload> {
    let candidate = PathBuf::from(&path)
        .canonicalize()
        .map_err(|e| CommandError::new("SCREENSHOT_NOT_FOUND", e.to_string()))?;
    let ext = candidate
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let allowed_ext = ["png", "jpg", "jpeg", "webp"].contains(&ext.as_str());
    let allowed = media_roots(&state, client)?
        .into_iter()
        .any(|root| within_root(&candidate, &root))
        && allowed_ext;
    if !allowed {
        return Err(CommandError::new(
            "SCREENSHOT_PATH_NOT_ALLOWED",
            "截图文件不在 osu! 数据目录内",
        ));
    }
    let metadata = fs::metadata(&candidate)
        .map_err(|error| CommandError::new("SCREENSHOT_READ_FAILED", error.to_string()))?;
    if metadata.len() > 64 * 1024 * 1024 {
        return Err(CommandError::new(
            "SCREENSHOT_TOO_LARGE",
            "截图文件超过 64 MB，请使用资源管理器直接查看",
        ));
    }
    let mime_type = match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => "image/png",
    }
    .into();
    let bytes = fs::read(&candidate)
        .map_err(|e| CommandError::new("SCREENSHOT_READ_FAILED", e.to_string()))?;
    Ok(GameScreenshotPayload {
        path: candidate.display().to_string(),
        file_name: candidate
            .file_name()
            .and_then(|x| x.to_str())
            .unwrap_or("screenshot.png")
            .into(),
        mime_type,
        bytes_base64: STANDARD.encode(bytes),
    })
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, path::Path};

    use super::{ReplayFingerprint, changed_replay_paths, same_executable};

    #[test]
    fn matches_windows_executables_without_case_sensitivity() {
        assert!(same_executable(
            Path::new("C:/Games/osu!/osu!.exe"),
            Path::new("c:/games/OSU!/OSU!.EXE")
        ));
    }

    #[test]
    fn replay_diff_includes_new_and_modified_files_only() {
        let fingerprint = |path: &str, size, modified_at_millis| ReplayFingerprint {
            path: path.into(),
            size,
            modified_at_millis,
        };
        let before = HashMap::from([
            ("same.osr".into(), fingerprint("same.osr", 10, 1)),
            ("changed.osr".into(), fingerprint("changed.osr", 10, 1)),
        ]);
        let current = HashMap::from([
            ("same.osr".into(), fingerprint("same.osr", 10, 1)),
            ("changed.osr".into(), fingerprint("changed.osr", 20, 2)),
            ("new.osr".into(), fingerprint("new.osr", 30, 3)),
        ]);
        assert_eq!(
            changed_replay_paths(&before, &current),
            vec!["changed.osr", "new.osr"]
        );
    }
}

#[tauri::command]
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

    crate::platform::reveal_path(&candidate)
        .map_err(|error| CommandError::new("EXPLORER_OPEN_FAILED", error.to_string()))?;
    Ok(())
}
