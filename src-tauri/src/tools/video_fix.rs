//! 修复导致 osu!stable 卡加载/闪退的谱面视频：复刻社区工具
//! osu-video-convert(mp42flv) 的方案。stable 对部分 MP4/M4V 封装（H.265、
//! VP9、AV1 等编码）会停在加载界面甚至闪退；这里先把视频无损重封装进 FLV
//! 容器（`-vcodec copy -an`，FLV 只接受 FLV1/VP6/H.264），编码不兼容时按
//! 原码率二次有损转码，最后用 FLV 内容原子替换原文件、保留原扩展名。
//! 相对参考实现的收紧：暂存文件写在原文件同目录并通过 rename 替换，覆盖前
//! 校验输出确实是非空 FLV，避免异常中断时用半成品覆盖原视频。

use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use walkdir::WalkDir;

use crate::{
    error::{CommandError, CommandResult},
    features::local_analysis::{LocalAnalysisService, LocalClient},
    infrastructure::{
        logging::{self, LogSpan},
        platform,
    },
    state::AppState,
};

/// 取消标志：同一时间只应有一个修复任务，模块级原子量足够。
static CANCELLED: AtomicBool = AtomicBool::new(false);
static RUNNING: AtomicBool = AtomicBool::new(false);
/// 当前 ffmpeg 子进程：二次转码可能耗时数分钟，取消命令需要跨线程终止它。
static FFMPEG_CHILD: Mutex<Option<Child>> = Mutex::new(None);

struct RunningGuard;
impl Drop for RunningGuard {
    fn drop(&mut self) {
        RUNNING.store(false, Ordering::Release);
    }
}

/// 参与修复的视频扩展名（与参考实现一致，大小写不敏感）。
const VIDEO_EXTENSIONS: [&str; 4] = ["avi", "mp4", "flv", "m4v"];
/// 暂存文件后缀：与原文件同目录，保证 rename 不跨分区且扫描不会误认。
const STAGING_SUFFIX: &str = ".opp-video-fix.tmp";
/// 扫描结果中待修复明细的展示上限，统计数字仍为完整值。
const MAX_PENDING_ENTRIES: usize = 2000;
/// 失败明细的展示上限，避免异常文件撑爆事件。
const MAX_FAILURES: usize = 50;
/// 等待 ffmpeg 退出时的轮询间隔。
const CHILD_POLL_INTERVAL: Duration = Duration::from_millis(120);

#[derive(Debug, Clone, Serialize)]
pub struct VideoFileEntry {
    pub path: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct VideoScanResult {
    pub songs_root: String,
    pub video_count: u64,
    pub video_size: u64,
    pub pending_count: u64,
    pub pending_size: u64,
    pub pending: Vec<VideoFileEntry>,
    pub pending_truncated: bool,
    pub already_flv_count: u64,
    pub empty_count: u64,
    pub other_count: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct VideoFixProgress {
    pub phase: &'static str,
    pub processed: usize,
    pub total: usize,
    pub percent: f64,
    pub current: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VideoFixFailure {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct VideoFixResult {
    pub songs_root: String,
    pub cancelled: bool,
    pub fixed_count: u64,
    pub fixed_size: u64,
    pub remuxed_count: u64,
    pub transcoded_count: u64,
    pub skipped_count: u64,
    pub failed_count: u64,
    pub failed: Vec<VideoFixFailure>,
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：扫描谱面目录中的视频文件并按容器类型分类。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn scan_osu_videos(
    root: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<VideoScanResult> {
    let span = logging::global().map(|logger| logger.operation("tools", "scan_osu_videos"));
    let service = Arc::clone(&state.local_analysis);
    let scanned = crate::infrastructure::tasks::background("tools", move || {
        scan(service.as_ref(), root.as_deref())
    })
    .await
    .map_err(|error| CommandError::new("VIDEO_SCAN_TASK_FAILED", error.to_string()))?;
    logging::finish_span(span, scanned)
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：修复需要转换的谱面视频。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn fix_osu_videos(
    root: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<VideoFixResult> {
    RUNNING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| CommandError::new("VIDEO_FIX_IN_PROGRESS", "已有视频修复任务正在进行"))?;
    let guard = RunningGuard;
    CANCELLED.store(false, Ordering::Relaxed);
    let ffmpeg = resolve_ffmpeg(&state).ok_or_else(|| {
        CommandError::new(
            "FFMPEG_REQUIRED",
            "未找到 FFmpeg：请在设置中指定 ffmpeg 路径，或安装到 PATH 后重试",
        )
    })?;
    let span = logging::global().map(|logger| logger.operation("tools", "fix_osu_videos"));
    if let Some(span) = span.as_ref() {
        span.info(
            "开始修复谱面视频",
            Some(serde_json::json!({ "ffmpeg": display(&ffmpeg), "root": root.as_deref() })),
        );
    }
    let service = Arc::clone(&state.local_analysis);
    let emit = Arc::new(move |progress: VideoFixProgress| {
        let _ = app.emit("video-fix-progress", progress);
    });
    crate::infrastructure::tasks::background("tools", move || {
        let _guard = guard;
        run_fix(service.as_ref(), root.as_deref(), ffmpeg, emit, span)
    })
    .await
    .map_err(|error| CommandError::new("VIDEO_FIX_TASK_FAILED", error.to_string()))?
}

#[tauri::command]
/// 供前端调用的 Tauri 命令：请求取消正在进行的修复任务。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn cancel_video_fix() {
    CANCELLED.store(true, Ordering::Relaxed);
    // 进行中的转码可能还要跑很久，直接把子进程结束掉。
    let Ok(mut slot) = FFMPEG_CHILD.lock() else {
        return;
    };
    if let Some(mut child) = slot.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn scan(service: &LocalAnalysisService, requested: Option<&str>) -> CommandResult<VideoScanResult> {
    let songs_root = resolve_songs_root(service, requested)?;
    let scanned = scan_videos(&songs_root, None);
    let mut result = VideoScanResult {
        songs_root: display(&songs_root),
        ..VideoScanResult::default()
    };
    for item in &scanned {
        result.video_count += 1;
        result.video_size += item.size;
        match item.kind {
            VideoKind::Mp4Container => {
                result.pending_count += 1;
                result.pending_size += item.size;
                if result.pending.len() < MAX_PENDING_ENTRIES {
                    result.pending.push(VideoFileEntry {
                        path: display(&item.path),
                        size: item.size,
                    });
                } else {
                    result.pending_truncated = true;
                }
            }
            VideoKind::AlreadyFlv => result.already_flv_count += 1,
            VideoKind::Empty => result.empty_count += 1,
            VideoKind::Other => result.other_count += 1,
        }
    }
    Ok(result)
}

fn run_fix(
    service: &LocalAnalysisService,
    requested: Option<&str>,
    ffmpeg: PathBuf,
    emit: Arc<dyn Fn(VideoFixProgress) + Send + Sync>,
    span: Option<LogSpan>,
) -> CommandResult<VideoFixResult> {
    let reporter = ProgressReporter::new(emit);
    let result = run_fix_inner(service, requested, &ffmpeg, &reporter, span.as_ref());
    logging::finish_span(span, result)
}

fn run_fix_inner(
    service: &LocalAnalysisService,
    requested: Option<&str>,
    ffmpeg: &Path,
    reporter: &ProgressReporter,
    span: Option<&LogSpan>,
) -> CommandResult<VideoFixResult> {
    let songs_root = resolve_songs_root(service, requested)?;
    fix_root(&songs_root, ffmpeg, reporter, span)
}

/// 对已解析的谱面目录执行扫描与修复。
fn fix_root(
    songs_root: &Path,
    ffmpeg: &Path,
    reporter: &ProgressReporter,
    span: Option<&LogSpan>,
) -> CommandResult<VideoFixResult> {
    let mut result = VideoFixResult {
        songs_root: display(songs_root),
        ..VideoFixResult::default()
    };
    if platform::game_process_running("stable") {
        return Err(CommandError::new(
            "STABLE_RUNNING",
            "检测到 osu!stable 正在运行，为避免与游戏读取竞争请先关闭它再执行",
        ));
    }

    // 预览与执行之间目录内容可能已经变化，修复前重新扫描一次。
    reporter.emit("scan", 0, 0, true, None);
    let scanned = scan_videos(songs_root, Some(reporter));
    if CANCELLED.load(Ordering::Relaxed) {
        result.cancelled = true;
        return Ok(result);
    }
    let mut pending = Vec::new();
    for item in scanned {
        match item.kind {
            VideoKind::Mp4Container => pending.push(item),
            _ => result.skipped_count += 1,
        }
    }

    let total = pending.len();
    for (index, item) in pending.iter().enumerate() {
        if CANCELLED.load(Ordering::Relaxed) {
            result.cancelled = true;
            break;
        }
        let started = Instant::now();
        let (remuxed, mode) = match fix_one(ffmpeg, &item.path, reporter, index, total) {
            FixOutcome::Remuxed => (true, "无损重封装"),
            FixOutcome::Transcoded => (false, "二次转码"),
            FixOutcome::Cancelled => {
                result.cancelled = true;
                break;
            }
            FixOutcome::Failed(message) => {
                if let Some(span) = span {
                    span.warn(
                        format!("修复视频失败：{}", display(&item.path)),
                        Some(serde_json::json!({ "message": message })),
                    );
                }
                record_failure(&mut result, &item.path, message);
                continue;
            }
        };
        if remuxed {
            result.remuxed_count += 1;
        } else {
            result.transcoded_count += 1;
        }
        result.fixed_count += 1;
        result.fixed_size += item.size;
        if let Some(span) = span {
            span.info(
                format!("{mode}完成：{}", display(&item.path)),
                Some(serde_json::json!({
                    "size": item.size,
                    "mode": mode,
                    "duration_ms": started.elapsed().as_millis(),
                })),
            );
        }
    }
    if !result.cancelled {
        // 最后一个文件用的是 0 起始序号，补一次终态让进度条收满。
        reporter.emit("copy", total, total, true, None);
    }
    Ok(result)
}

fn record_failure(result: &mut VideoFixResult, path: &Path, message: String) {
    result.failed_count += 1;
    if result.failed.len() < MAX_FAILURES {
        result.failed.push(VideoFixFailure {
            path: display(path),
            message,
        });
    }
}

/// 单文件修复结果：取消与普通失败需要分开处理。
enum FixOutcome {
    Remuxed,
    Transcoded,
    Cancelled,
    Failed(String),
}

/// 修复单个视频：先尝试无损重封装，输出不是 FLV 时按原码率二次转码；
/// 暂存输出通过同目录 rename 原子替换原文件。
fn fix_one(
    ffmpeg: &Path,
    path: &Path,
    reporter: &ProgressReporter,
    index: usize,
    total: usize,
) -> FixOutcome {
    let staging = staging_path(path);
    let _cleanup = StagingGuard(staging.clone());
    let _ = fs::remove_file(&staging);

    // 第一遍：无损重封装（`-vcodec copy -an`），H.264 视频直接换壳。
    reporter.emit("copy", index, total, false, Some(display(path)));
    let mut command = ffmpeg_command(ffmpeg, true);
    command.arg("-i").arg(path);
    command
        .args(["-vcodec", "copy", "-an", "-f", "flv"])
        .arg(&staging);
    let first = match run_ffmpeg(&mut command) {
        Ok(outcome) => outcome,
        Err(message) => return FixOutcome::Failed(message),
    };
    let Some(first) = first else {
        return FixOutcome::Cancelled;
    };
    if is_valid_flv(&staging) {
        return match fs::rename(&staging, path) {
            Ok(()) => FixOutcome::Remuxed,
            Err(error) => FixOutcome::Failed(format!("无法写回原文件：{error}")),
        };
    }

    // 编码与 FLV 不兼容（HEVC/VP9/AV1 等），读取原码率后二次转码。
    let _ = fs::remove_file(&staging);
    let bitrate = match probe_bitrate(ffmpeg, path) {
        Ok(ProbeOutcome::Bitrate(bitrate)) => bitrate,
        Ok(ProbeOutcome::Unavailable) => {
            return FixOutcome::Failed(format!(
                "视频编码与 FLV 不兼容且读不到码率，未执行二次转码（{}）",
                stderr_tail(&first.stderr)
            ));
        }
        Ok(ProbeOutcome::Cancelled) => return FixOutcome::Cancelled,
        Err(message) => return FixOutcome::Failed(message),
    };
    reporter.emit("transcode", index, total, false, Some(display(path)));
    let mut command = ffmpeg_command(ffmpeg, true);
    command.arg("-i").arg(path);
    command
        .args(["-b:v", &format!("{bitrate}k"), "-an", "-f", "flv"])
        .arg(&staging);
    let second = match run_ffmpeg(&mut command) {
        Ok(outcome) => outcome,
        Err(message) => return FixOutcome::Failed(message),
    };
    let Some(second) = second else {
        return FixOutcome::Cancelled;
    };
    if !second.status.success() || !is_valid_flv(&staging) {
        return FixOutcome::Failed(format!("二次转码失败（{}）", stderr_tail(&second.stderr)));
    }
    match fs::rename(&staging, path) {
        Ok(()) => FixOutcome::Transcoded,
        Err(error) => FixOutcome::Failed(format!("无法写回原文件：{error}")),
    }
}

/// 暂存文件清理守卫：失败与取消路径都不会留下半成品。
struct StagingGuard(PathBuf);
impl Drop for StagingGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

struct FfmpegOutcome {
    status: ExitStatus,
    stderr: String,
}

/// 组装 ffmpeg 调用：静默、无交互、覆盖输出，Windows 下不弹出窗口。
/// 探测输入信息需要 info 级日志，因此 quiet 为 false 时不加 `-loglevel`。
fn ffmpeg_command(ffmpeg: &Path, quiet: bool) -> Command {
    let mut command = Command::new(ffmpeg);
    command.args(["-hide_banner", "-nostdin", "-y"]);
    if quiet {
        command.args(["-loglevel", "error"]);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
}

/// 运行 ffmpeg 并等待结束。子进程在等待期间登记到 `FFMPEG_CHILD`，
/// 取消命令可以随时终止它；返回 `Ok(None)` 表示等待期间任务被取消。
fn run_ffmpeg(command: &mut Command) -> Result<Option<FfmpegOutcome>, String> {
    let mut child = command
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => "未找到 FFmpeg 可执行文件".to_string(),
            _ => format!("无法启动 FFmpeg：{error}"),
        })?;
    let reader = child.stderr.take().map(|mut pipe| {
        std::thread::spawn(move || {
            let mut buffer = String::new();
            let _ = pipe.read_to_string(&mut buffer);
            buffer
        })
    });
    {
        let mut slot = FFMPEG_CHILD
            .lock()
            .map_err(|_| "内部状态锁已损坏".to_string())?;
        *slot = Some(child);
    }
    let status = loop {
        std::thread::sleep(CHILD_POLL_INTERVAL);
        let mut slot = FFMPEG_CHILD
            .lock()
            .map_err(|_| "内部状态锁已损坏".to_string())?;
        let Some(child) = slot.as_mut() else {
            // 取消命令已取走子进程并终止它。
            return Ok(None);
        };
        match child.try_wait() {
            Ok(Some(status)) => {
                *slot = None;
                break status;
            }
            Ok(None) => {}
            Err(error) => {
                *slot = None;
                return Err(format!("等待 FFmpeg 进程失败：{error}"));
            }
        }
    };
    let stderr = reader
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    Ok(Some(FfmpegOutcome { status, stderr }))
}

/// 码率探测结果：取消需要与「读不到码率」区分开。
enum ProbeOutcome {
    Bitrate(u64),
    Unavailable,
    Cancelled,
}

/// 用 `ffmpeg -i <input>`（无输出文件）读取媒体信息，从 stderr 的
/// `Duration: ..., bitrate: N kb/s` 行解析原视频码率。ffmpeg 因为没有输出
/// 文件会以非零状态退出，这是参考实现同样依赖的行为。
fn probe_bitrate(ffmpeg: &Path, path: &Path) -> Result<ProbeOutcome, String> {
    let mut command = ffmpeg_command(ffmpeg, false);
    command.arg("-i").arg(path);
    let outcome = run_ffmpeg(&mut command)?;
    let Some(outcome) = outcome else {
        return Ok(ProbeOutcome::Cancelled);
    };
    Ok(match parse_bitrate(&outcome.stderr) {
        Some(bitrate) => ProbeOutcome::Bitrate(bitrate),
        None => ProbeOutcome::Unavailable,
    })
}

/// 从 ffmpeg 的输入信息里解析码率（kb/s），没有码率信息（`N/A`）时返回 None。
fn parse_bitrate(stderr: &str) -> Option<u64> {
    let duration_line = stderr.lines().find(|line| line.contains("Duration:"))?;
    let (_, rest) = duration_line.split_once("bitrate:")?;
    let digits: String = rest
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok().filter(|value| *value > 0)
}

/// 输出必须是「非空 + FLV 魔数」才会覆盖原文件：参考实现只检查非空，
/// 这里收紧以避免异常中断时用半成品覆盖原视频。
fn is_valid_flv(path: &Path) -> bool {
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    if file.metadata().map(|metadata| metadata.len()).unwrap_or(0) == 0 {
        return false;
    }
    let mut magic = [0u8; 3];
    file.read_exact(&mut magic).is_ok() && &magic == b"FLV"
}

/// 取 stderr 尾部作为失败原因：ffmpeg 的错误信息通常集中在最后几行。
fn stderr_tail(stderr: &str) -> String {
    const LIMIT: usize = 400;
    let trimmed = stderr.trim();
    if trimmed.len() <= LIMIT {
        return trimmed.to_string();
    }
    let mut start = trimmed.len() - LIMIT;
    while !trimmed.is_char_boundary(start) {
        start += 1;
    }
    format!("…{}", &trimmed[start..])
}

/// 容器类型按文件头前 4 字节判定。参考实现把「前 3 字节全为 0」视为仍未
/// 修复的 MP4 系容器（ISO BMFF 的 box size 前缀），FLV 头以 `FLV` 开头，
/// AVI 等其余容器与空文件一样跳过。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VideoKind {
    Mp4Container,
    AlreadyFlv,
    Empty,
    Other,
}

fn classify_video(path: &Path) -> std::io::Result<VideoKind> {
    let mut file = File::open(path)?;
    let mut header = [0u8; 4];
    let read = file.read(&mut header)?;
    if read < 4 {
        return Ok(VideoKind::Empty);
    }
    Ok(match header {
        [0x46, 0x4C, 0x56, _] => VideoKind::AlreadyFlv,
        [0, 0, 0, _] => VideoKind::Mp4Container,
        _ => VideoKind::Other,
    })
}

fn has_video_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            VIDEO_EXTENSIONS
                .iter()
                .any(|item| item.eq_ignore_ascii_case(extension))
        })
}

/// 暂存路径与原文件同目录：保证 rename 原子替换且不跨分区。扩展名刻意不用
/// 视频后缀（扫描不会误认），ffmpeg 输出因此需要显式 `-f flv`。
fn staging_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "video".into());
    path.with_file_name(format!("{name}{STAGING_SUFFIX}"))
}

struct ScannedVideo {
    path: PathBuf,
    size: u64,
    kind: VideoKind,
}

/// 遍历谱面目录（Songs/<地图>/<文件> 两层）收集视频文件并按头字节分类。
fn scan_videos(root: &Path, reporter: Option<&ProgressReporter>) -> Vec<ScannedVideo> {
    let mut scanned = Vec::new();
    let mut walked = 0usize;
    for entry in WalkDir::new(root)
        .max_depth(2)
        .into_iter()
        .filter_map(Result::ok)
    {
        if CANCELLED.load(Ordering::Relaxed) {
            return scanned;
        }
        if !entry.file_type().is_file() || !has_video_extension(entry.path()) {
            continue;
        }
        let Ok(size) = entry.metadata().map(|metadata| metadata.len()) else {
            continue;
        };
        walked += 1;
        // 进度上报自带节流，逐文件调用即可。
        if let Some(reporter) = reporter {
            reporter.emit("scan", walked, 0, false, None);
        }
        let kind = match classify_video(entry.path()) {
            Ok(kind) => kind,
            Err(error) => {
                crate::log_warn!(
                    "tools",
                    "读取视频文件头失败: {} ({error})",
                    display(entry.path())
                );
                VideoKind::Other
            }
        };
        scanned.push(ScannedVideo {
            path: entry.into_path(),
            size,
            kind,
        });
    }
    if let Some(reporter) = reporter {
        reporter.emit("scan", walked, 0, true, None);
    }
    scanned
}

/// 解析待扫描的谱面目录：优先使用请求中用户选择的路径，否则读取已配置的
/// stable 谱面目录。
fn resolve_songs_root(
    service: &LocalAnalysisService,
    requested: Option<&str>,
) -> CommandResult<PathBuf> {
    if let Some(requested) = requested.map(str::trim).filter(|value| !value.is_empty()) {
        return songs_root_from_request(requested);
    }
    let resolved = service.resolved_source(LocalClient::Stable)?;
    if let Some(beatmaps) = resolved.beatmap_root.as_ref().filter(|path| path.is_dir()) {
        return Ok(beatmaps.clone());
    }
    let detail = if resolved.status.validation_errors.is_empty() {
        "未找到 Songs 谱面目录".to_string()
    } else {
        resolved.status.validation_errors.join("；")
    };
    Err(CommandError::new(
        "STABLE_NOT_FOUND",
        format!("未找到可扫描的 osu!stable 谱面目录：{detail}"),
    ))
}

/// 用户选择的目录：直接选中 osu! 根目录时自动下沉到 Songs 子目录，
/// 与参考实现「选择 osu! 根目录」的交互保持一致。
fn songs_root_from_request(requested: &str) -> CommandResult<PathBuf> {
    let selected = PathBuf::from(requested.trim());
    let songs = selected.join("Songs");
    let path = if songs.is_dir() { songs } else { selected };
    if path.is_dir() {
        Ok(path)
    } else {
        Err(CommandError::new(
            "VIDEO_ROOT_NOT_FOUND",
            "所选目录不存在或不可读取",
        ))
    }
}

/// FFmpeg 解析：设置中手动路径 > PATH 自动检测 > danser 发行包自带。
fn resolve_ffmpeg(state: &State<'_, AppState>) -> Option<PathBuf> {
    let saved = state.store.settings_snapshot().ok().map(|snapshot| {
        (
            snapshot.ffmpeg_executable_path.clone(),
            snapshot.danser_executable_path.clone(),
        )
    });
    let (ffmpeg, danser) = saved.unzip();
    crate::features::danser::resolve_ffmpeg_path(
        ffmpeg.flatten().as_deref(),
        danser.flatten().as_deref(),
    )
}

struct ProgressReporter {
    emit: Arc<dyn Fn(VideoFixProgress) + Send + Sync>,
    last_emit: Mutex<Instant>,
}

impl ProgressReporter {
    fn new(emit: Arc<dyn Fn(VideoFixProgress) + Send + Sync>) -> Self {
        Self {
            emit,
            last_emit: Mutex::new(Instant::now() - Duration::from_secs(1)),
        }
    }

    fn emit(
        &self,
        phase: &'static str,
        processed: usize,
        total: usize,
        force: bool,
        current: Option<String>,
    ) {
        let Ok(mut last_emit) = self.last_emit.lock() else {
            return;
        };
        if !force && last_emit.elapsed() < Duration::from_millis(100) {
            return;
        }
        *last_emit = Instant::now();
        drop(last_emit);
        let percent = if total > 0 {
            ((processed as f64 / total as f64) * 1000.0).round() / 10.0
        } else {
            0.0
        };
        (self.emit)(VideoFixProgress {
            phase,
            processed,
            total,
            percent: percent.min(100.0),
            current,
        });
    }
}

fn display(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_video_containers_by_header() {
        let directory = tempfile::tempdir().expect("temp directory");
        let write = |name: &str, bytes: &[u8]| {
            let path = directory.path().join(name);
            fs::write(&path, bytes).expect("write file");
            path
        };
        let mp4 = write("video.mp4", &[0, 0, 0, 0x18, b'f', b't', b'y', b'p']);
        assert_eq!(
            classify_video(&mp4).expect("classify"),
            VideoKind::Mp4Container
        );
        let flv = write("fixed.mp4", b"FLV\x01\x05");
        assert_eq!(
            classify_video(&flv).expect("classify"),
            VideoKind::AlreadyFlv
        );
        let avi = write("video.avi", b"RIFF\x08\x00\x00\x00AVI ");
        assert_eq!(classify_video(&avi).expect("classify"), VideoKind::Other);
        let empty = write("empty.mp4", b"");
        assert_eq!(classify_video(&empty).expect("classify"), VideoKind::Empty);
    }

    #[test]
    fn parses_bitrate_from_duration_line() {
        let stderr = "  Duration: 00:03:24.32, start: 0.000000, bitrate: 1175 kb/s\n";
        assert_eq!(parse_bitrate(stderr), Some(1175));
        assert_eq!(
            parse_bitrate("  Duration: N/A, start: N/A, bitrate: N/A\n"),
            None
        );
        assert_eq!(parse_bitrate("no duration here"), None);
    }

    #[test]
    fn matches_video_extensions_case_insensitively() {
        assert!(has_video_extension(Path::new("video.MP4")));
        assert!(has_video_extension(Path::new("video.m4v")));
        assert!(!has_video_extension(Path::new("chart.osu")));
        assert!(!has_video_extension(Path::new(
            "video.mp4.opp-video-fix.tmp"
        )));
    }

    #[test]
    fn derives_songs_directory_from_selected_root() {
        let directory = tempfile::tempdir().expect("temp directory");
        let songs = directory.path().join("Songs");
        fs::create_dir_all(&songs).expect("create songs");
        let root = directory.path().to_string_lossy().into_owned();
        assert_eq!(songs_root_from_request(&root).expect("resolve"), songs);
        assert!(songs_root_from_request("Z:/definitely/missing").is_err());
    }

    #[test]
    fn builds_staging_path_next_to_source() {
        let path = Path::new("map").join("video.mp4");
        assert_eq!(
            staging_path(&path),
            Path::new("map").join("video.mp4.opp-video-fix.tmp")
        );
    }

    /// ffmpeg 子进程登记在模块级静态量里，ffmpeg 端到端测试必须串行执行。
    fn serial() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// 用 lavfi 生成一个测试视频；libx264 等外部编码器缺失时返回 false。
    fn generate_video(ffmpeg: &Path, destination: &Path, encoder: &str) -> bool {
        let mut command = Command::new(ffmpeg);
        command.args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=1:size=64x64:rate=5",
            "-c:v",
            encoder,
        ]);
        if encoder == "libx264" {
            command.args(["-preset", "ultrafast"]);
        }
        command
            .arg(destination)
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    /// 有 ffmpeg 时做一次真实修复：mpeg4 编码与 FLV 不兼容，走二次转码路径。
    #[test]
    fn transcodes_incompatible_video_into_flv_in_place() {
        let _serial = serial();
        let Some(ffmpeg) = platform::find_in_path("ffmpeg") else {
            return;
        };
        let directory = tempfile::tempdir().expect("temp directory");
        let input = directory.path().join("video.mp4");
        assert!(generate_video(&ffmpeg, &input, "mpeg4"), "生成测试视频失败");
        assert_eq!(
            classify_video(&input).expect("classify"),
            VideoKind::Mp4Container
        );

        CANCELLED.store(false, Ordering::Relaxed);
        let reporter = ProgressReporter::new(Arc::new(|_| {}));
        match fix_one(&ffmpeg, &input, &reporter, 0, 1) {
            FixOutcome::Transcoded => {}
            FixOutcome::Failed(message) => panic!("转换失败: {message}"),
            _ => panic!("未预期的转换结果"),
        }
        assert!(is_valid_flv(&input), "输出应替换为 FLV 内容");
        assert!(!staging_path(&input).exists(), "暂存文件应被清理");
    }

    /// 有 ffmpeg 与 libx264 时验证无损路径：H.264 视频只换容器不重编码。
    #[test]
    fn remuxes_h264_video_without_reencoding() {
        let _serial = serial();
        let Some(ffmpeg) = platform::find_in_path("ffmpeg") else {
            return;
        };
        let directory = tempfile::tempdir().expect("temp directory");
        let input = directory.path().join("video.mp4");
        if !generate_video(&ffmpeg, &input, "libx264") {
            // 发行版未编译 libx264 时跳过：无损路径需要 H.264 输入。
            return;
        }

        CANCELLED.store(false, Ordering::Relaxed);
        let reporter = ProgressReporter::new(Arc::new(|_| {}));
        match fix_one(&ffmpeg, &input, &reporter, 0, 1) {
            FixOutcome::Remuxed => {}
            FixOutcome::Failed(message) => panic!("重封装失败: {message}"),
            _ => panic!("未预期的转换结果"),
        }
        assert!(is_valid_flv(&input), "输出应替换为 FLV 内容");
        assert!(!staging_path(&input).exists(), "暂存文件应被清理");
    }

    /// 完整流程：扫描谱面目录、修复待处理视频、跳过其余文件并正确计数。
    #[test]
    fn fixes_pending_videos_and_skips_others() {
        let _serial = serial();
        let Some(ffmpeg) = platform::find_in_path("ffmpeg") else {
            return;
        };
        let directory = tempfile::tempdir().expect("temp directory");
        let songs = directory.path().join("Songs");
        let remux = songs.join("100 map/video.mp4");
        let transcode = songs.join("200 map/background.m4v");
        let already = songs.join("300 map/fixed.mp4");
        let empty = songs.join("400 map/broken.mp4");
        for path in [&remux, &transcode, &already, &empty] {
            fs::create_dir_all(path.parent().expect("parent")).expect("create map directory");
        }
        if !generate_video(&ffmpeg, &remux, "libx264") {
            return;
        }
        assert!(
            generate_video(&ffmpeg, &transcode, "mpeg4"),
            "生成测试视频失败"
        );
        fs::write(&already, b"FLV\x01\x05").expect("write flv");
        fs::write(&empty, b"").expect("write empty");
        fs::write(songs.join("200 map/chart.osu"), b"osu file format v14")
            .expect("write beatmap file");

        CANCELLED.store(false, Ordering::Relaxed);
        let reporter = ProgressReporter::new(Arc::new(|_| {}));
        let result = fix_root(&songs, &ffmpeg, &reporter, None).expect("fix songs");
        assert_eq!(result.fixed_count, 2);
        assert_eq!(result.remuxed_count, 1);
        assert_eq!(result.transcoded_count, 1);
        assert_eq!(result.skipped_count, 2);
        assert_eq!(result.failed_count, 0);
        assert!(!result.cancelled);
        assert!(is_valid_flv(&remux), "无损路径输出应为 FLV");
        assert!(is_valid_flv(&transcode), "转码路径输出应为 FLV");
        assert_eq!(fs::read(&empty).expect("read empty"), b"");
    }
}
