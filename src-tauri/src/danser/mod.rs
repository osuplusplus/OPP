mod discovery;
mod models;
#[cfg(test)]
mod tests;

use std::{
    collections::VecDeque,
    fs,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, atomic::Ordering, mpsc},
    time::{Duration, SystemTime},
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use chrono::Utc;
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use crate::{
    error::{CommandError, CommandResult},
    game_session::load_game_replay_file,
    app::models::DanserRenderPreferences,
    app::state::AppState,
};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

use discovery::{ffmpeg_available, find_danser, list_profiles_for, resolve_danser_path};
pub use models::DanserRuntime;
use models::{
    DanserEnqueueRequest, DanserRenderJob, DanserRenderProgress, DanserStatus, DanserTask,
};

fn command_error(code: &str, message: impl Into<String>) -> CommandError {
    CommandError::new(code, message.into())
}

fn resolve_danser(state: &AppState) -> CommandResult<PathBuf> {
    let saved = state.store.snapshot()?.settings.danser_executable_path;
    resolve_danser_path(saved.as_deref()).ok_or_else(|| {
        command_error(
            "DANSER_NOT_FOUND",
            "未找到 danser，请在 PATH 中提供或在设置中选择 Danser 程序",
        )
    })
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：读取当前状态或详情。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn get_danser_status(state: State<'_, AppState>) -> CommandResult<DanserStatus> {
    let saved = state.store.snapshot()?.settings.danser_executable_path;
    let executable = find_danser(saved.as_deref());
    let profiles = executable
        .as_deref()
        .map(list_profiles_for)
        .unwrap_or_default();
    let ffmpeg = executable.as_deref().is_some_and(ffmpeg_available);
    Ok(DanserStatus {
        available: executable.is_some(),
        executable_path: executable.as_ref().map(|path| path.display().to_string()),
        ffmpeg_available: ffmpeg,
        profiles,
        message: match (&executable, ffmpeg) {
            (None, _) => "未检测到 Danser".into(),
            (Some(_), false) => "已检测到 Danser，但未找到 FFmpeg".into(),
            (Some(_), true) => "Danser 与 FFmpeg 已就绪".into(),
        },
    })
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：列出可用资源。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn list_danser_profiles(state: State<'_, AppState>) -> CommandResult<Vec<String>> {
    Ok(list_profiles_for(&resolve_danser(&state)?))
}

fn validate_preferences(input: &DanserRenderPreferences) -> CommandResult<()> {
    if !(0.1..=4.0).contains(&input.speed) || !(0.1..=4.0).contains(&input.pitch) {
        return Err(command_error(
            "INVALID_DANSER_SPEED",
            "速度和音高必须在 0.1 到 4.0 之间",
        ));
    }
    if let (Some(start), Some(end)) = (input.start, input.end)
        && (start < 0.0 || end <= start)
    {
        return Err(command_error(
            "INVALID_DANSER_RANGE",
            "结束时间必须大于开始时间",
        ));
    }
    if !input.mods.trim().is_empty() && !input.mods2.trim().is_empty() {
        return Err(command_error(
            "DANSER_MODS_CONFLICT",
            "mods 与 mods2 不能同时设置",
        ));
    }
    if !input.mods2.trim().is_empty() {
        serde_json::from_str::<serde_json::Value>(input.mods2.trim()).map_err(|error| {
            command_error("INVALID_DANSER_MODS2", format!("mods2 JSON 无效：{error}"))
        })?;
    }
    if !input.settings_patch.trim().is_empty() {
        let value = serde_json::from_str::<serde_json::Value>(input.settings_patch.trim())
            .map_err(|error| {
                command_error("INVALID_DANSER_PATCH", format!("sPatch JSON 无效：{error}"))
            })?;
        if !value.is_object() {
            return Err(command_error(
                "INVALID_DANSER_PATCH",
                "sPatch 必须是 JSON 对象",
            ));
        }
    }
    if !(320..=7680).contains(&input.frame_width)
        || !(240..=4320).contains(&input.frame_height)
        || !(15..=480).contains(&input.fps)
    {
        return Err(command_error(
            "INVALID_DANSER_RECORDING_SIZE",
            "视频尺寸或帧率超出支持范围",
        ));
    }
    if !matches!(
        input.encoder.as_str(),
        "libx264" | "h264_nvenc" | "h264_qsv"
    ) {
        return Err(command_error(
            "INVALID_DANSER_ENCODER",
            "不支持的 Danser 视频编码器",
        ));
    }
    if input.quality > 51 || !(1..=64).contains(&input.motion_blur_oversample) {
        return Err(command_error(
            "INVALID_DANSER_RECORDING_QUALITY",
            "视频质量或运动模糊采样值无效",
        ));
    }
    Ok(())
}

fn safe_stem(path: &str) -> String {
    let original = Path::new(path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("replay");
    let value: String = original
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || matches!(character, '-' | '_' | ' ') {
                character
            } else {
                '_'
            }
        })
        .collect();
    let value = value.trim_matches([' ', '.', '_']);
    if value.is_empty() {
        "replay".into()
    } else {
        value.chars().take(80).collect()
    }
}

fn unique_output_name(directory: &Path, replay_path: &str) -> String {
    let base = format!(
        "{}-{}",
        safe_stem(replay_path),
        Utc::now().format("%Y%m%d-%H%M%S")
    );
    for suffix in 0..1000 {
        let name = if suffix == 0 {
            base.clone()
        } else {
            format!("{base}-{suffix}")
        };
        let occupied = fs::read_dir(directory)
            .ok()
            .into_iter()
            .flatten()
            .flatten()
            .any(|entry| {
                entry.path().file_stem().and_then(|value| value.to_str()) == Some(name.as_str())
            });
        if !occupied {
            return name;
        }
    }
    format!("{base}-{}", Uuid::new_v4())
}

fn merge_json(target: &mut serde_json::Value, patch: serde_json::Value) {
    match (target, patch) {
        (serde_json::Value::Object(target), serde_json::Value::Object(patch)) => {
            for (key, value) in patch {
                merge_json(target.entry(key).or_insert(serde_json::Value::Null), value);
            }
        }
        (target, patch) => *target = patch,
    }
}

fn runtime_settings_patch(task: &DanserTask) -> CommandResult<String> {
    let replay = Path::new(&task.replay_path);
    let replay_directory = replay.parent().unwrap_or(Path::new("."));
    let osu_root = replay_directory.parent().unwrap_or(replay_directory);
    let songs = osu_root.join("Songs");
    let skins = osu_root.join("Skins");
    let mut patch = serde_json::json!({
        "General": {
            "OsuSongsDir": songs,
            "OsuSkinsDir": skins,
            "OsuReplaysDir": replay_directory,
        },
        "Recording": {
            "FrameWidth": task.preferences.frame_width,
            "FrameHeight": task.preferences.frame_height,
            "FPS": task.preferences.fps,
            "Encoder": task.preferences.encoder,
            "MotionBlur": {
                "Enabled": task.preferences.motion_blur,
                "OversampleMultiplier": task.preferences.motion_blur_oversample,
                "BlendFrames": task.preferences.motion_blur_oversample.saturating_mul(3) / 2,
            },
        }
    });
    let quality_key = match task.preferences.encoder.as_str() {
        "h264_nvenc" => "CQ",
        "h264_qsv" => "Quality",
        _ => "CRF",
    };
    patch["Recording"][task.preferences.encoder.as_str()][quality_key] =
        serde_json::json!(task.preferences.quality);
    if !task.preferences.settings_patch.trim().is_empty() {
        let user_patch =
            serde_json::from_str(task.preferences.settings_patch.trim()).map_err(|error| {
                command_error("INVALID_DANSER_PATCH", format!("sPatch JSON 无效：{error}"))
            })?;
        merge_json(&mut patch, user_patch);
    }
    serde_json::to_string(&patch)
        .map_err(|error| command_error("DANSER_PATCH_FAILED", error.to_string()))
}

fn build_arguments(task: &DanserTask, output: &Path) -> CommandResult<Vec<String>> {
    validate_preferences(&task.preferences)?;
    let input = &task.preferences;
    let mut args = vec![
        format!("-replay={}", task.replay_path),
        "-record".into(),
        "-preciseprogress".into(),
        format!("-out={}", output.display()),
    ];
    // Linux：录制输出目录只能写进 settings 文件（见 ensure_opp_profile），统一用
    // OPP 维护的 opp.json 启动；用户选择的 profile 作为其内容来源。Windows 沿用
    // 发行包自带 profile。
    #[cfg(not(windows))]
    args.push("-settings=opp".into());
    #[cfg(windows)]
    if !input.settings_profile.trim().is_empty() {
        args.push(format!("-settings={}", input.settings_profile.trim()));
    }
    if !input.skin.trim().is_empty() {
        args.push(format!("-skin={}", input.skin.trim()));
    }
    if input.quickstart {
        args.push("-quickstart".into());
    } else if input.skip {
        args.push("-skip".into());
    }
    if let Some(value) = input.start {
        args.push(format!("-start={value}"));
    }
    if let Some(value) = input.end {
        args.push(format!("-end={value}"));
    }
    if (input.speed - 1.0).abs() > f64::EPSILON {
        args.push(format!("-speed={}", input.speed));
    }
    if (input.pitch - 1.0).abs() > f64::EPSILON {
        args.push(format!("-pitch={}", input.pitch));
    }
    if input.offset != 0 {
        args.push(format!("-offset={}", input.offset));
    }
    if !input.mods.trim().is_empty() {
        args.push(format!("-mods={}", input.mods.trim()));
    }
    if !input.mods2.trim().is_empty() {
        args.push(format!("-mods2={}", input.mods2.trim()));
    }
    for (name, value) in [
        ("cs", input.cs),
        ("ar", input.ar),
        ("od", input.od),
        ("hp", input.hp),
    ] {
        if let Some(value) = value {
            args.push(format!("-{name}={value}"));
        }
    }
    if input.no_db_check {
        args.push("-nodbcheck".into());
    }
    if input.no_update_check {
        args.push("-noupdatecheck".into());
    }
    if input.debug {
        args.push("-debug".into());
    }
    args.push(format!("-sPatch={}", runtime_settings_patch(task)?));
    Ok(args)
}

fn parse_progress(line: &str) -> Option<u8> {
    let percent = line.find('%')?;
    let digits: String = line[..percent]
        .chars()
        .rev()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    digits.parse::<u8>().ok().map(|value| value.min(100))
}

fn forward_lines<R: Read + Send + 'static>(stream: Option<R>, sender: mpsc::Sender<String>) {
    if let Some(stream) = stream {
        std::thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                let _ = sender.send(line);
            }
        });
    }
}

fn find_output(stem: &Path, started: SystemTime) -> Option<PathBuf> {
    let parent = stem.parent()?;
    let name = stem.file_name()?.to_str()?;
    fs::read_dir(parent)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let matches = path.file_stem().and_then(|value| value.to_str()) == Some(name);
            let modified = entry.metadata().ok()?.modified().ok()?;
            (matches && modified >= started).then_some((modified, path))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}

fn export_output(source: &Path, directory: &Path) -> CommandResult<PathBuf> {
    let file_name = source
        .file_name()
        .ok_or_else(|| command_error("DANSER_OUTPUT_NOT_FOUND", "Danser 输出文件名无效"))?;
    let target = directory.join(file_name);
    // Linux 上 OutputDir 就指向导出目录，产物已在目标位置，无需转存（同路径的
    // rename/copy 反而可能损坏文件）。
    if target == source {
        return Ok(target);
    }
    match fs::rename(source, &target) {
        Ok(()) => Ok(target),
        Err(_) => {
            fs::copy(source, &target).map_err(|error| {
                command_error(
                    "DANSER_EXPORT_FAILED",
                    format!("无法转存 Danser 视频：{error}"),
                )
            })?;
            fs::remove_file(source).map_err(|error| {
                command_error(
                    "DANSER_EXPORT_FAILED",
                    format!("视频已导出，但无法清理 Danser 临时文件：{error}"),
                )
            })?;
            Ok(target)
        }
    }
}

fn failure_detail(lines: &VecDeque<String>, fallback: impl ToString) -> String {
    const IMPORTANT: [&str; 4] = ["panic:", "fatal", "error:", "failed"];
    lines
        .iter()
        .rev()
        .find(|line| {
            let lower = line.to_ascii_lowercase();
            IMPORTANT.iter().any(|needle| lower.contains(needle))
        })
        .or_else(|| lines.iter().rev().find(|line| !line.trim().is_empty()))
        .map(|line| line.trim().to_string())
        .unwrap_or_else(|| fallback.to_string())
}

fn emit(app: &AppHandle, job: &DanserRenderJob) {
    let _ = app.emit(
        "danser-render-progress",
        DanserRenderProgress {
            id: job.id.clone(),
            replay_path: job.replay_path.clone(),
            status: job.status.clone(),
            progress: job.progress,
            description: job.description.clone(),
            output_path: job.output_path.clone(),
            queue_position: job.queue_position,
        },
    );
}

fn update_job(
    runtime: &DanserRuntime,
    app: &AppHandle,
    id: &str,
    update: impl FnOnce(&mut DanserRenderJob),
) {
    if let Ok(mut jobs) = runtime.jobs.lock()
        && let Some(job) = jobs.iter_mut().find(|job| job.id == id)
    {
        update(job);
        emit(app, job);
    }
}

fn remove_cancelled_job(runtime: &DanserRuntime, app: &AppHandle, id: &str) {
    let removed = runtime.jobs.lock().ok().and_then(|mut jobs| {
        let index = jobs.iter().position(|job| job.id == id)?;
        Some(jobs.remove(index))
    });
    if let Some(mut job) = removed {
        job.status = "cancelled".into();
        job.description = "已从队列移除".into();
        job.queue_position = None;
        emit(app, &job);
    }
}

/// Linux：danser-go 的 settings 在 XDG 配置目录（`~/.config/danser`）。录制输出
/// 目录 `Recording.OutputDir` 无法经 `-sPatch` 传入、`-out` 只给文件名，只能写进
/// settings 文件：把用户当前 profile 复制成专用 `opp.json` 并写入绝对 OutputDir，
/// 启动时用 `-settings=opp`，不改动用户自己的配置。
#[cfg(not(windows))]
fn ensure_opp_profile(
    preferences: &DanserRenderPreferences,
    output_dir: &Path,
) -> CommandResult<()> {
    let config_dir = crate::platform::danser_config_dir()
        .ok_or_else(|| command_error("DANSER_CONFIG_DIR_NOT_FOUND", "无法确定 danser 配置目录"))?;
    fs::create_dir_all(&config_dir)?;
    let requested = preferences.settings_profile.trim();
    let requested = if requested.is_empty() {
        "default"
    } else {
        requested
    };
    // 首选用户选择的 profile，缺失时回退 default.json，再缺失用空对象（danser 按
    // 内置默认值补全其余字段）。
    let mut value = fs::read(config_dir.join(format!("{requested}.json")))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .or_else(|| {
            fs::read(config_dir.join("default.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        })
        .unwrap_or_else(|| serde_json::json!({}));
    merge_json(
        &mut value,
        serde_json::json!({ "Recording": { "OutputDir": output_dir.display().to_string() } }),
    );
    let serialized = serde_json::to_vec_pretty(&value)
        .map_err(|error| command_error("DANSER_PROFILE_WRITE_FAILED", error.to_string()))?;
    fs::write(config_dir.join("opp.json"), serialized).map_err(|error| {
        command_error(
            "DANSER_PROFILE_WRITE_FAILED",
            format!("无法写入 danser 配置文件：{error}"),
        )
    })?;
    Ok(())
}

fn execute_task(
    runtime: &DanserRuntime,
    app: &AppHandle,
    task: &DanserTask,
    executable: &Path,
    export_directory: &Path,
) {
    let output_name = unique_output_name(export_directory, &task.replay_path);
    // Linux：danser 的输出目录由 settings 的 Recording.OutputDir 决定
    #[cfg(not(windows))]
    if let Err(error) = ensure_opp_profile(&task.preferences, export_directory) {
        update_job(runtime, app, &task.id, |job| {
            job.status = "failed".into();
            job.description = error.message;
        });
        return;
    }
    let args = match build_arguments(task, Path::new(&output_name)) {
        Ok(args) => args,
        Err(error) => {
            update_job(runtime, app, &task.id, |job| {
                job.status = "failed".into();
                job.description = error.message;
            });
            return;
        }
    };
    update_job(runtime, app, &task.id, |job| {
        job.status = "running".into();
        job.progress = 0;
        job.queue_position = None;
        job.description = "Danser 正在准备渲染".into();
    });
    let started = SystemTime::now();
    // 避免 danser 向只读的系统安装目录写入。
    #[cfg(not(windows))]
    let working_dir = export_directory.to_path_buf();
    #[cfg(windows)]
    let working_dir = executable.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut command = Command::new(executable);
    command
        .args(args)
        .current_dir(working_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            update_job(runtime, app, &task.id, |job| {
                job.status = "failed".into();
                job.description = format!("无法启动 Danser：{error}");
            });
            return;
        }
    };
    let (sender, receiver) = mpsc::channel::<String>();
    forward_lines(child.stdout.take(), sender.clone());
    forward_lines(child.stderr.take(), sender.clone());
    drop(sender);
    let mut recent = VecDeque::new();
    let mut last_progress = None;
    loop {
        while let Ok(line) = receiver.try_recv() {
            if recent.len() >= 80 {
                recent.pop_front();
            }
            recent.push_back(line.clone());
            if let Some(progress) = parse_progress(&line)
                && last_progress != Some(progress)
            {
                last_progress = Some(progress);
                update_job(runtime, app, &task.id, |job| {
                    job.progress = progress;
                    job.description = format!("本地渲染 {progress}%");
                });
            }
        }
        let cancelled = runtime
            .cancelled
            .lock()
            .map(|items| items.contains(&task.id))
            .unwrap_or(false);
        if cancelled {
            let _ = child.kill();
            let _ = child.wait();
            remove_cancelled_job(runtime, app, &task.id);
            if let Ok(mut cancelled) = runtime.cancelled.lock() {
                cancelled.remove(&task.id);
            }
            return;
        }
        match child.try_wait() {
            Ok(Some(status)) if status.success() => {
                // Linux：OutputDir 已指向导出目录，产物直接落在这里；
                // Windows：danser 发行包的 videos/ 子目录。
                #[cfg(not(windows))]
                let render_stem = export_directory.join(&output_name);
                #[cfg(windows)]
                let render_stem = executable
                    .parent()
                    .unwrap_or(Path::new("."))
                    .join("videos")
                    .join(&output_name);
                if let Some(path) = find_output(&render_stem, started) {
                    match export_output(&path, export_directory) {
                        Ok(exported) => update_job(runtime, app, &task.id, |job| {
                            job.status = "completed".into();
                            job.progress = 100;
                            job.description = "本地视频已导出".into();
                            job.output_path = Some(exported.display().to_string());
                        }),
                        Err(error) => update_job(runtime, app, &task.id, |job| {
                            job.status = "failed".into();
                            job.description = error.message;
                        }),
                    }
                } else {
                    update_job(runtime, app, &task.id, |job| {
                        job.status = "failed".into();
                        job.description = "Danser 已退出，但未找到输出视频".into();
                    });
                }
                return;
            }
            Ok(Some(status)) => {
                let detail = failure_detail(&recent, format!("退出代码 {status}"));
                update_job(runtime, app, &task.id, |job| {
                    job.status = "failed".into();
                    job.description = format!("Danser 渲染失败：{detail}");
                });
                return;
            }
            Err(error) => {
                update_job(runtime, app, &task.id, |job| {
                    job.status = "failed".into();
                    job.description = format!("无法读取 Danser 状态：{error}");
                });
                return;
            }
            _ => std::thread::sleep(Duration::from_millis(180)),
        }
    }
}

fn start_worker(
    runtime: Arc<DanserRuntime>,
    app: AppHandle,
    executable: PathBuf,
    export_directory: PathBuf,
) {
    if runtime.worker_running.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn_blocking(move || {
        loop {
            let task = runtime
                .queue
                .lock()
                .ok()
                .and_then(|mut queue| queue.pop_front());
            let Some(task) = task else {
                runtime.worker_running.store(false, Ordering::SeqCst);
                if runtime
                    .queue
                    .lock()
                    .map(|queue| queue.is_empty())
                    .unwrap_or(true)
                    || runtime.worker_running.swap(true, Ordering::SeqCst)
                {
                    return;
                }
                continue;
            };
            execute_task(&runtime, &app, &task, &executable, &export_directory);
            if let Ok(mut jobs) = runtime.jobs.lock() {
                let waiting: Vec<String> = runtime
                    .queue
                    .lock()
                    .map(|queue| queue.iter().map(|task| task.id.clone()).collect())
                    .unwrap_or_default();
                for job in jobs.iter_mut() {
                    job.queue_position = waiting
                        .iter()
                        .position(|id| id == &job.id)
                        .map(|index| index + 1);
                }
            }
        }
    });
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：完成该功能模块的业务操作。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn enqueue_danser_renders(
    request: DanserEnqueueRequest,
    state: State<'_, AppState>,
    app: AppHandle,
) -> CommandResult<Vec<DanserRenderJob>> {
    if request.replay_paths.is_empty() {
        return Err(command_error("NO_REPLAYS_SELECTED", "请选择至少一个回放"));
    }
    validate_preferences(&request.preferences)?;
    let executable = resolve_danser(&state)?;
    if !ffmpeg_available(&executable) {
        return Err(command_error("FFMPEG_NOT_FOUND", "Danser 无法访问 FFmpeg"));
    }
    let export_directory = state
        .store
        .snapshot()?
        .settings
        .replay_export_directory
        .map(PathBuf::from)
        .ok_or_else(|| {
            command_error(
                "REPLAY_EXPORT_DIRECTORY_NOT_SET",
                "请先在设置中选择回放导出位置",
            )
        })?;
    fs::create_dir_all(&export_directory)?;
    let mut created = Vec::new();
    for replay_path in &request.replay_paths {
        let bytes = load_game_replay_file(request.client, replay_path, &state)?;
        if bytes.first().copied() != Some(0) {
            return Err(command_error(
                "DANSER_RULESET_UNSUPPORTED",
                "Danser 仅支持 osu!standard 回放",
            ));
        }
        let id = Uuid::new_v4().to_string();
        let position = state
            .danser
            .queue
            .lock()
            .map(|queue| queue.len() + 1)
            .unwrap_or(1);
        let job = DanserRenderJob {
            id: id.clone(),
            replay_path: replay_path.clone(),
            status: "queued".into(),
            progress: 0,
            description: "已加入队列，等待开始".into(),
            output_path: None,
            queue_position: Some(position),
        };
        state
            .danser
            .queue
            .lock()
            .map_err(|_| command_error("DANSER_QUEUE_LOCKED", "Danser 队列不可用"))?
            .push_back(DanserTask {
                id,
                replay_path: replay_path.clone(),
                preferences: request.preferences.clone(),
            });
        state
            .danser
            .jobs
            .lock()
            .map_err(|_| command_error("DANSER_QUEUE_LOCKED", "Danser 队列不可用"))?
            .push(job.clone());
        emit(&app, &job);
        created.push(job);
    }
    Ok(created)
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：启动后台任务或外部服务。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn start_danser_render_queue(state: State<'_, AppState>, app: AppHandle) -> CommandResult<()> {
    let has_waiting = !state
        .danser
        .queue
        .lock()
        .map_err(|_| command_error("DANSER_QUEUE_LOCKED", "Danser 队列不可用"))?
        .is_empty();
    if !has_waiting {
        return Err(command_error("DANSER_QUEUE_EMPTY", "队列中没有待渲染任务"));
    }
    let executable = resolve_danser(&state)?;
    if !ffmpeg_available(&executable) {
        return Err(command_error("FFMPEG_NOT_FOUND", "Danser 无法访问 FFmpeg"));
    }
    let export_directory = state
        .store
        .snapshot()?
        .settings
        .replay_export_directory
        .map(PathBuf::from)
        .ok_or_else(|| command_error("REPLAY_EXPORT_DIRECTORY_NOT_SET", "请先选择回放导出位置"))?;
    fs::create_dir_all(&export_directory)?;
    start_worker(state.danser.clone(), app, executable, export_directory);
    Ok(())
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：读取当前状态或详情。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn get_danser_render_queue(state: State<'_, AppState>) -> CommandResult<Vec<DanserRenderJob>> {
    state
        .danser
        .jobs
        .lock()
        .map(|jobs| jobs.iter().rev().take(100).cloned().collect())
        .map_err(|_| command_error("DANSER_QUEUE_LOCKED", "Danser 队列不可用"))
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：请求取消正在进行的任务。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn cancel_danser_render(
    id: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> CommandResult<()> {
    let removed_from_queue = if let Ok(mut queue) = state.danser.queue.lock() {
        let found = queue.iter().any(|task| task.id == id);
        queue.retain(|task| task.id != id);
        found
    } else {
        false
    };
    if !removed_from_queue {
        state
            .danser
            .cancelled
            .lock()
            .map_err(|_| command_error("DANSER_QUEUE_LOCKED", "Danser 队列不可用"))?
            .insert(id.clone());
    }
    remove_cancelled_job(&state.danser, &app, &id);
    Ok(())
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：在系统中打开资源或输出位置。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn open_danser_output(path: String, state: State<'_, AppState>) -> CommandResult<()> {
    let directory = state
        .store
        .snapshot()?
        .settings
        .replay_export_directory
        .map(PathBuf::from)
        .ok_or_else(|| command_error("REPLAY_EXPORT_DIRECTORY_NOT_SET", "尚未设置回放导出目录"))?;
    let directory = directory
        .canonicalize()
        .map_err(|error| command_error("EXPORT_DIRECTORY_NOT_FOUND", error.to_string()))?;
    let target = PathBuf::from(&path)
        .canonicalize()
        .map_err(|error| command_error("DANSER_OUTPUT_NOT_FOUND", error.to_string()))?;
    if !target.starts_with(&directory) {
        return Err(command_error(
            "DANSER_OUTPUT_NOT_ALLOWED",
            "输出文件不在回放导出目录中",
        ));
    }
    crate::platform::reveal_path(&target)
        .map_err(|error| command_error("OPEN_OUTPUT_FAILED", error.to_string()))?;
    Ok(())
}
