use std::{
    collections::VecDeque,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
};

use chrono::Utc;
use futures_util::StreamExt;
use tauri::{AppHandle, Emitter};
use tokio_tungstenite::connect_async;
use url::Url;

use crate::{
    error::{CommandError, CommandResult},
    models::AppSettings,
};

use super::models::{TosuLiveSnapshot, TosuLogEntry};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const MAX_LOG_LINES: usize = 500;

pub struct TosuRuntime {
    tosu_process: Mutex<Option<Child>>,
    lyrics_process: Mutex<Option<Child>>,
    logs: Mutex<VecDeque<TosuLogEntry>>,
    live: Mutex<Option<TosuLiveSnapshot>>,
    connecting: std::sync::atomic::AtomicBool,
    last_error: Mutex<Option<String>>,
}

impl Default for TosuRuntime {
    fn default() -> Self {
        Self {
            tosu_process: Mutex::new(None),
            lyrics_process: Mutex::new(None),
            logs: Mutex::new(VecDeque::new()),
            live: Mutex::new(None),
            connecting: std::sync::atomic::AtomicBool::new(false),
            last_error: Mutex::new(None),
        }
    }
}

pub fn normalize_base_url(input: &str) -> CommandResult<String> {
    let mut url = Url::parse(input.trim())
        .map_err(|_| CommandError::new("INVALID_TOSU_API_URL", "tosu API 地址无效"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(CommandError::new(
            "INVALID_TOSU_API_URL",
            "tosu API 地址必须是 HTTP(S) 基地址",
        ));
    }
    url.set_path("");
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.to_string().trim_end_matches('/').to_owned())
}

pub fn executable_path(settings: &AppSettings) -> CommandResult<PathBuf> {
    let raw = settings.tosu_executable_path.as_deref().ok_or_else(|| {
        CommandError::new("TOSU_NOT_CONFIGURED", "请先在工具 > tosu 中选择 tosu.exe")
    })?;
    validate_executable(Path::new(raw))
}

pub fn lyrics_executable_path(settings: &AppSettings) -> CommandResult<PathBuf> {
    // 与 tosu 一致：Linux 优先使用 PATH 中的 tosu-proxy，回退用户配置；Windows
    // 仅使用用户配置。
    #[cfg(not(windows))]
    if let Some(found) = crate::platform::find_in_path("tosu-proxy") {
        return Ok(found);
    }
    let raw = settings
        .tosu_lyrics_executable_path
        .as_deref()
        .ok_or_else(|| {
            CommandError::new(
                "TOSU_LYRICS_NOT_CONFIGURED",
                "请先选择 tosu-proxy 可执行文件",
            )
        })?;
    validate_lyrics_executable(Path::new(raw))
}

pub fn validate_executable(path: &Path) -> CommandResult<PathBuf> {
    // 仅接受存在的可执行文件；后续启动使用规范化路径避免配置中的相对路径漂移。
    let path = path.canonicalize().map_err(|_| {
        CommandError::new("TOSU_EXECUTABLE_NOT_FOUND", "未找到所选的 tosu 可执行文件")
    })?;
    let valid_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            #[cfg(windows)]
            {
                name.eq_ignore_ascii_case("tosu.exe")
            }
            #[cfg(not(windows))]
            {
                name.eq_ignore_ascii_case("tosu")
            }
        });
    if !path.is_file() || !valid_name {
        return Err(CommandError::new(
            "INVALID_TOSU_EXECUTABLE",
            "请选择官方发行包中的 tosu 可执行文件",
        ));
    }
    Ok(path)
}

pub fn validate_lyrics_executable(path: &Path) -> CommandResult<PathBuf> {
    let path = path.canonicalize().map_err(|_| {
        CommandError::new(
            "TOSU_LYRICS_EXECUTABLE_NOT_FOUND",
            "未找到所选的 tosu-lyrics 代理可执行文件",
        )
    })?;
    let valid_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            #[cfg(windows)]
            {
                name.eq_ignore_ascii_case("tosu-proxy.exe")
            }
            #[cfg(not(windows))]
            {
                name.eq_ignore_ascii_case("tosu-proxy")
            }
        });
    if !path.is_file() || !valid_name {
        return Err(CommandError::new(
            "INVALID_TOSU_LYRICS_EXECUTABLE",
            "请选择 tosu-lyrics 发行包中的 tosu-proxy 可执行文件",
        ));
    }
    Ok(path)
}

fn record(runtime: &TosuRuntime, app: &AppHandle, stream: &str, message: impl Into<String>) {
    let message = message.into();
    let lower = message.to_ascii_lowercase();
    let level = if lower.contains("error") || lower.contains("failed") || lower.contains("panic") {
        "error"
    } else if stream.contains("stderr") || lower.contains("warn") {
        "warning"
    } else {
        "info"
    };
    let entry = TosuLogEntry {
        at: Utc::now(),
        stream: stream.into(),
        level: level.into(),
        message,
    };
    if let Ok(mut logs) = runtime.logs.lock() {
        logs.push_back(entry.clone());
        if logs.len() > MAX_LOG_LINES {
            logs.pop_front();
        }
    }
    let _ = app.emit("tosu-log", entry);
}

pub fn log_system(runtime: &TosuRuntime, app: &AppHandle, message: impl Into<String>) {
    record(runtime, app, "system", message);
}

fn decode_log_line(bytes: &[u8]) -> String {
    let bytes = bytes
        .strip_suffix(b"\n")
        .unwrap_or(bytes)
        .strip_suffix(b"\r")
        .unwrap_or(bytes);
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_owned();
    }
    let mut detector = chardetng::EncodingDetector::new();
    detector.feed(bytes, true);
    let encoding = detector.guess(None, true);
    let (text, _, _) = encoding.decode(bytes);
    text.into_owned()
}

fn read_output<R: std::io::Read + Send + 'static>(
    reader: R,
    stream: &'static str,
    runtime: Arc<TosuRuntime>,
    app: AppHandle,
) {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        let mut bytes = Vec::new();
        loop {
            bytes.clear();
            match reader.read_until(b'\n', &mut bytes) {
                Ok(0) => break,
                Ok(_) => {
                    let line = decode_log_line(&bytes);
                    if !line.trim().is_empty() {
                        record(&runtime, &app, stream, line);
                    }
                }
                Err(error) => {
                    record(
                        &runtime,
                        &app,
                        "system",
                        format!("读取 tosu {stream} 日志失败：{error}"),
                    );
                    break;
                }
            }
        }
    });
}

pub fn is_owned_running(runtime: &TosuRuntime) -> bool {
    is_process_running(&runtime.tosu_process)
}

pub fn is_lyrics_owned_running(runtime: &TosuRuntime) -> bool {
    is_process_running(&runtime.lyrics_process)
}

fn is_process_running(process: &Mutex<Option<Child>>) -> bool {
    process
        .lock()
        .ok()
        .and_then(|mut child| {
            child
                .as_mut()
                .and_then(|child| child.try_wait().ok().flatten())
                .map(|_| false)
                .or_else(|| child.as_ref().map(|_| true))
        })
        .unwrap_or(false)
}

pub fn process_running() -> bool {
    #[cfg(windows)]
    {
        named_process_running("tosu.exe", "tosu")
    }
    #[cfg(not(windows))]
    {
        !crate::platform::tosu_process_ids().is_empty()
    }
}

pub fn lyrics_process_running() -> bool {
    named_process_running("tosu-proxy.exe", "tosu-proxy")
}

fn named_process_running(windows_name: &str, unix_name: &str) -> bool {
    #[cfg(windows)]
    {
        let _ = unix_name;
        Command::new("tasklist")
            .creation_flags(CREATE_NO_WINDOW)
            .args(["/FI", &format!("IMAGENAME eq {windows_name}"), "/NH"])
            .output()
            .map(|output| {
                String::from_utf8_lossy(&output.stdout)
                    .to_ascii_lowercase()
                    .contains(&windows_name.to_ascii_lowercase())
            })
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        crate::platform::unix_process_running(unix_name)
    }
}

/// 把路径安全地嵌入 bash 脚本（单引号包裹，内部单引号转义）。
#[cfg(not(windows))]
fn shell_word(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', r"'\''"))
}

/// 以提权方式运行 tosu（Linux 下读取 wine 进程内存需要 root）。
///
/// 经 `pkexec`（PolicyKit）图形弹窗输密码，后台以 root 看门狗脚本运行：脚本
/// 轮询停止标志文件（OPP 创建即停止，无需二次认证）和 OPP 的 PID（OPP 退出
/// 或崩溃则自动终止 tosu）。
#[cfg(not(windows))]
fn spawn_elevated_tosu(
    runtime: &TosuRuntime,
    app: &AppHandle,
    executable: &Path,
) -> CommandResult<()> {
    if crate::platform::find_in_path("pkexec").is_none() {
        return Err(CommandError::new(
            "PKEXEC_NOT_FOUND",
            "未找到 pkexec（PolicyKit），无法提权启动 tosu",
        ));
    }
    let flag = crate::platform::tosu_stop_flag();
    let _ = std::fs::remove_file(&flag);
    // 看门狗：tosu 死了就退出；停止标志出现或 OPP 不在了就杀 tosu
    // （先 TERM，1 秒后仍存活则 KILL）。
    let script = format!(
        "F={flag}; rm -f \"$F\"; '{exe}' & P=$!; while kill -0 \"$P\" 2>/dev/null; do \
         if [ -e \"$F\" ] || ! kill -0 {opp} 2>/dev/null; then \
         kill \"$P\" 2>/dev/null; sleep 1; kill -9 \"$P\" 2>/dev/null; break; fi; sleep 1; done; \
         rm -f \"$F\"; wait \"$P\" 2>/dev/null",
        flag = shell_word(&flag),
        exe = executable.display().to_string(),
        opp = std::process::id(),
    );
    let mut child = Command::new("pkexec")
        .args(["bash", "-c", &script])
        .spawn()
        .map_err(|error| {
            CommandError::new(
                "TOSU_START_FAILED",
                format!("无法通过 pkexec 启动 tosu：{error}"),
            )
        })?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    record(
        runtime,
        app,
        "system",
        "已通过 pkexec 提权启动 tosu，请在系统弹窗中输入密码。tosu 由看门狗托管：OPP 退出时自动停止，也可在页面直接终止。",
    );
    Ok(())
}

fn start_lyrics_if_configured(runtime: Arc<TosuRuntime>, settings: &AppSettings, app: AppHandle) {
    if !settings.launch_tosu_lyrics_with_tosu || is_lyrics_owned_running(&runtime) {
        return;
    }
    if lyrics_process_running() {
        record(
            &runtime,
            &app,
            "lyrics:system",
            "检测到外部启动的 tosu-lyrics 代理。",
        );
        return;
    }
    let executable = match lyrics_executable_path(settings) {
        Ok(path) => path,
        Err(_) => {
            record(
                &runtime,
                &app,
                "lyrics:system",
                "tosu-lyrics 未配置，已跳过歌词代理启动。",
            );
            return;
        }
    };
    // 找到就跟着 tosu 直接以普通子进程启动，不额外管理生命周期/权限。
    let mut command = Command::new(&executable);
    command
        .current_dir(executable.parent().unwrap_or(Path::new(".")))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    match command.spawn() {
        Ok(mut child) => {
            if let Some(stdout) = child.stdout.take() {
                read_output(stdout, "lyrics:stdout", runtime.clone(), app.clone());
            }
            if let Some(stderr) = child.stderr.take() {
                read_output(stderr, "lyrics:stderr", runtime.clone(), app.clone());
            }
            if let Ok(mut process) = runtime.lyrics_process.lock() {
                *process = Some(child);
            }
            record(
                &runtime,
                &app,
                "lyrics:system",
                "tosu-lyrics 代理已由 OPP 在后台启动。访问 http://127.0.0.1:41280/lyrics/",
            );
        }
        Err(error) => record(
            &runtime,
            &app,
            "lyrics:system",
            format!("无法启动 tosu-lyrics 代理：{error}"),
        ),
    }
}

pub fn start(
    runtime: Arc<TosuRuntime>,
    settings: &AppSettings,
    app: AppHandle,
) -> CommandResult<()> {
    if is_owned_running(&runtime) {
        return Ok(());
    }
    if process_running() {
        record(
            &runtime,
            &app,
            "system",
            "检测到外部启动的 tosu，OPP 将直接连接其 API。",
        );
        start_lyrics_if_configured(runtime.clone(), settings, app.clone());
        ensure_live_connection(runtime, settings.tosu_api_base_url.clone(), app);
        return Ok(());
    }
    // Linux 下 tosu 需要读取 wine 进程内存（root），经 pkexec 图形授权提权启动：
    // 优先使用 PATH 中的 tosu，其次使用用户配置的路径
    #[cfg(not(windows))]
    {
        let candidate =
            crate::platform::find_in_path("tosu").or_else(|| executable_path(settings).ok());
        let Some(executable) = candidate else {
            return Err(CommandError::new(
                "TOSU_NOT_IN_PATH",
                "未在 PATH 中找到 tosu，且未配置有效的 tosu 路径",
            ));
        };
        spawn_elevated_tosu(&runtime, &app, &executable)?;
        start_lyrics_if_configured(runtime.clone(), settings, app.clone());
        ensure_live_connection(runtime, settings.tosu_api_base_url.clone(), app);
        return Ok(());
    }
    #[cfg(windows)]
    {
        let executable = executable_path(settings)?;
        let mut command = Command::new(&executable);
        command
            .current_dir(executable.parent().unwrap_or(Path::new(".")))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command.creation_flags(CREATE_NO_WINDOW);
        let mut child = command.spawn().map_err(|error| {
            CommandError::new("TOSU_START_FAILED", format!("无法启动 tosu：{error}"))
        })?;
        if let Some(stdout) = child.stdout.take() {
            read_output(stdout, "stdout", runtime.clone(), app.clone());
        }
        if let Some(stderr) = child.stderr.take() {
            read_output(stderr, "stderr", runtime.clone(), app.clone());
        }
        *runtime
            .tosu_process
            .lock()
            .map_err(|_| CommandError::new("TOSU_PROCESS_LOCKED", "tosu 进程状态不可用"))? =
            Some(child);
        record(&runtime, &app, "system", "tosu 已由 OPP 在后台启动。");
        start_lyrics_if_configured(runtime.clone(), settings, app.clone());
        ensure_live_connection(runtime, settings.tosu_api_base_url.clone(), app);
        Ok(())
    }
}

/// 停止由 OPP 启动的 tosu-lyrics 代理（普通子进程，直接 kill）
fn stop_owned_lyrics(runtime: &TosuRuntime, app: &AppHandle) {
    if let Ok(mut lyrics) = runtime.lyrics_process.lock()
        && let Some(mut child) = lyrics.take()
    {
        let _ = child.kill();
        let _ = child.wait();
        record(
            runtime,
            app,
            "lyrics:system",
            "已停止由 OPP 启动的 tosu-lyrics 代理。",
        );
    }
}

/// OPP 退出时清理：终止由 OPP 启动的 tosu-lyrics 子进程
pub fn cleanup_on_exit(runtime: &TosuRuntime) {
    // 仅终止本进程启动的 lyrics 代理，避免退出 OPP 时误杀外部 tosu。
    if let Ok(mut lyrics) = runtime.lyrics_process.lock()
        && let Some(mut child) = lyrics.take()
    {
        let _ = child.kill();
        let _ = child.wait();
    }
}

pub fn stop(runtime: &TosuRuntime, app: &AppHandle) -> CommandResult<()> {
    let mut guard = runtime
        .tosu_process
        .lock()
        .map_err(|_| CommandError::new("TOSU_PROCESS_LOCKED", "tosu 进程状态不可用"))?;
    let Some(mut child) = guard.take() else {
        drop(guard);
        // Linux 上经 pkexec 提权启动的 tosu 不归 OPP 管理，需再次提权终止；
        // 歌词代理是 OPP 的普通子进程，先直接停掉
        #[cfg(not(windows))]
        {
            stop_owned_lyrics(runtime, app);
            return stop_external_tosu(runtime, app);
        }
        #[cfg(windows)]
        {
            let _ = app;
            return Err(CommandError::new(
                "TOSU_NOT_OWNED",
                "OPP 只能停止由自身启动的 tosu",
            ));
        }
    };
    drop(guard);
    let _ = child.kill();
    let _ = child.wait();
    stop_owned_lyrics(runtime, app);
    record(runtime, app, "system", "已停止由 OPP 启动的 tosu。");
    Ok(())
}

/// 终止外部（提权）运行的 tosu。优先写停止标志——pkexec 看门狗脚本 1 秒内
/// 轮询到即杀 tosu，无需再次认证；超时（非看门狗托管或脚本失效）才回退
/// `pkexec kill` 图形授权。
#[cfg(not(windows))]
fn stop_external_tosu(runtime: &TosuRuntime, app: &AppHandle) -> CommandResult<()> {
    let pids = crate::platform::tosu_process_ids();
    if pids.is_empty() {
        return Err(CommandError::new(
            "TOSU_NOT_RUNNING",
            "未检测到正在运行的 tosu",
        ));
    }
    let flag = crate::platform::tosu_stop_flag();
    if std::fs::write(&flag, b"").is_ok() {
        for _ in 0..40 {
            if crate::platform::tosu_process_ids().is_empty() {
                record(
                    runtime,
                    app,
                    "system",
                    "已通过停止标志终止 tosu（无需再次认证）。",
                );
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let _ = std::fs::remove_file(&flag);
    }
    let pid_args = pids.iter().map(u32::to_string).collect::<Vec<_>>();
    if crate::platform::find_in_path("pkexec").is_none() {
        return Err(CommandError::new(
            "PKEXEC_NOT_FOUND",
            "未找到 pkexec（PolicyKit），无法提权终止 tosu",
        ));
    }
    // pkexec 用绝对路径更稳；pkexec 会弹图形授权框，等待期间不阻塞 UI
    // （stop_tosu 命令在 blocking 线程中调用此处）。
    let kill = crate::platform::find_in_path("kill")
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "kill".into());
    let output = Command::new("pkexec")
        .arg(&kill)
        .args(&pid_args)
        .output()
        .map_err(|error| {
            CommandError::new("TOSU_STOP_FAILED", format!("无法执行 pkexec kill：{error}"))
        })?;
    if !output.status.success() {
        return Err(CommandError::new(
            "TOSU_STOP_FAILED",
            format!(
                "终止 tosu 失败：{}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ));
    }
    record(
        runtime,
        app,
        "system",
        format!("已终止外部运行的 tosu（PID：{}）。", pid_args.join("、")),
    );
    Ok(())
}

pub fn logs(runtime: &TosuRuntime) -> Vec<TosuLogEntry> {
    runtime
        .logs
        .lock()
        .map(|logs| logs.iter().cloned().collect())
        .unwrap_or_default()
}

pub fn last_error(runtime: &TosuRuntime) -> Option<String> {
    runtime
        .last_error
        .lock()
        .ok()
        .and_then(|error| error.clone())
}

pub fn ensure_live_connection(runtime: Arc<TosuRuntime>, base: String, app: AppHandle) {
    use std::sync::atomic::Ordering;
    if runtime.connecting.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let endpoint = match normalize_base_url(&base).and_then(|base| {
            let mut url = Url::parse(&base)
                .map_err(|_| CommandError::new("INVALID_TOSU_API_URL", "tosu API 地址无效"))?;
            url.set_scheme(if url.scheme() == "https" { "wss" } else { "ws" })
                .map_err(|_| CommandError::new("INVALID_TOSU_API_URL", "tosu API 地址无效"))?;
            url.set_path("/websocket/v2");
            Ok::<_, CommandError>(url.to_string())
        }) {
            Ok(endpoint) => endpoint,
            Err(error) => {
                *runtime
                    .last_error
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(error.message);
                runtime.connecting.store(false, Ordering::SeqCst);
                return;
            }
        };
        match connect_async(&endpoint).await {
            Ok((mut stream, _)) => {
                *runtime
                    .last_error
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
                record(&runtime, &app, "system", "已连接到 tosu v2 实时 API。");
                while let Some(message) = stream.next().await {
                    match message {
                        Ok(message) if message.is_text() => {
                            match serde_json::from_str::<serde_json::Value>(
                                message.to_text().unwrap_or_default(),
                            ) {
                                Ok(value) => {
                                    let snapshot = TosuLiveSnapshot::from_v2(&value);
                                    if let Ok(mut current) = runtime.live.lock() {
                                        *current = Some(snapshot.clone());
                                    }
                                    let _ = app.emit("tosu-live-data", snapshot);
                                }
                                Err(error) => record(
                                    &runtime,
                                    &app,
                                    "system",
                                    format!("无法解析 tosu 实时数据：{error}"),
                                ),
                            }
                        }
                        Ok(_) => {}
                        Err(error) => {
                            *runtime
                                .last_error
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                                Some(error.to_string());
                            record(
                                &runtime,
                                &app,
                                "system",
                                format!("tosu 实时连接已断开：{error}"),
                            );
                            break;
                        }
                    }
                }
            }
            Err(error) => {
                *runtime
                    .last_error
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(error.to_string());
                record(
                    &runtime,
                    &app,
                    "system",
                    format!("无法连接 tosu API：{error}"),
                );
            }
        }
        runtime.connecting.store(false, Ordering::SeqCst);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalizes_api_urls() {
        assert_eq!(
            normalize_base_url("http://127.0.0.1:24050/").unwrap(),
            "http://127.0.0.1:24050"
        );
    }
    #[test]
    fn rejects_non_http_urls() {
        assert!(normalize_base_url("ws://127.0.0.1:24050").is_err());
    }

    #[test]
    fn decodes_non_utf8_windows_log_output() {
        let (bytes, _, _) = encoding_rs::GBK.encode("歌词代理已启动");
        assert_eq!(decode_log_line(&bytes), "歌词代理已启动");
    }
}
