//! 结构化 JSONL 日志设施。
//!
//! 所有日志先脱敏再经有界通道交给专用写盘线程，业务线程不会阻塞在文件 IO 上；
//! 队列满时普通日志按计数合并补报、错误级日志落应急文件，
//! 历史日志按修改时间轮转，仅保留最近 5 份。
use chrono::Utc;
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, SyncSender, TrySendError},
    },
    thread,
    time::{Duration, Instant},
};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;
use uuid::Uuid;

use crate::error::CommandError;
use crate::error::CommandResult;

/// 可克隆的日志句柄：只持有通道发送端与应急文件路径，写盘由后台线程完成。
#[derive(Clone)]
pub struct Logger {
    tx: SyncSender<WriterMessage>,
    // 队列满时被丢弃的普通日志计数，由写盘线程定期合并成一条补报。
    dropped: Arc<AtomicU64>,
    // 队列满或写盘线程已退出时，紧急日志直接追加到该应急文件。
    fallback: Arc<Mutex<PathBuf>>,
    directory: PathBuf,
}

/// 写盘线程的消息：`Record` 携带序列化后的行与“是否需立即落盘”的紧急标记，
/// `Shutdown` 携带应答通道，写盘线程排空后回执，用于优雅退出。
enum WriterMessage {
    Record(String, bool),
    Shutdown(mpsc::Sender<()>),
}

// 全局唯一实例：init 之后常驻；未初始化时 global() 返回 None，调用方静默跳过日志。
static LOGGER: OnceLock<Logger> = OnceLock::new();
// 保证 panic 钩子只安装一次，重复 init 不会重复链挂或覆盖前一个钩子。
static PANIC_HOOK: OnceLock<()> = OnceLock::new();

/// 单条 JSONL 记录的序列化结构；可选字段缺省时不输出键，保持每行紧凑。
#[derive(Serialize)]
struct LogRecord {
    timestamp: String,
    level: String,
    target: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    event: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fields: Option<serde_json::Value>,
}

/// 事件附加信息：相对 span 起点的耗时（毫秒）与任意结构化字段。
#[derive(Default)]
struct EventDetails {
    duration_ms: Option<u128>,
    fields: Option<serde_json::Value>,
}

impl EventDetails {
    fn new(duration_ms: Option<u128>, fields: Option<serde_json::Value>) -> Self {
        Self {
            duration_ms,
            fields,
        }
    }
}

/// 初始化日志系统：创建目录、轮转旧文件、启动写盘线程并安装 panic 钩子。
///
/// 文件名带时间戳与短 UUID，避免同秒多次启动互相覆盖；各步失败一律不阻断应用启动。
pub fn init(app_data_dir: &Path) -> Logger {
    let directory = app_data_dir.join("logs");
    let _ = fs::create_dir_all(&directory);
    prune(&directory);
    let current = format!(
        "opp-{}-{}.jsonl",
        Utc::now().format("%Y%m%d-%H%M%S"),
        &Uuid::new_v4().to_string()[..8]
    );
    let path = directory.join(&current);
    let fallback = Arc::new(Mutex::new(path.with_extension("emergency.jsonl")));
    let (tx, rx) = mpsc::sync_channel::<WriterMessage>(1024);
    let dropped = Arc::new(AtomicU64::new(0));
    let lost = dropped.clone();
    let _ = thread::Builder::new()
        .name("opp-log-writer".into())
        .spawn(move || write_records(path, rx, lost));
    let logger = Logger {
        tx,
        dropped,
        fallback,
        directory,
    };
    let _ = LOGGER.set(logger.clone());
    install_panic_hook();
    logger
}

/// 写盘线程主循环：普通记录累计 64 条、空闲 250ms 或遇到紧急记录才 flush，
/// 在刷盘频率与宕机丢失风险之间取折中；收到 `Shutdown` 时最终刷新并退出。
///
/// 每轮醒来先把丢弃计数合并补报成一条告警；目标文件打不开时全部退回 stderr。
fn write_records(path: PathBuf, rx: mpsc::Receiver<WriterMessage>, lost: Arc<AtomicU64>) {
    use std::io::{BufWriter, Write};
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .ok()
        .map(BufWriter::new);
    let mut pending = 0;
    loop {
        let message = rx.recv_timeout(Duration::from_millis(250));
        let count = lost.swap(0, Ordering::Relaxed);
        if count > 0 {
            let line = serde_json::json!({"timestamp": Utc::now().to_rfc3339(), "level": "WARN", "target": "logging", "message": "日志队列已满，合并普通日志", "fields": {"dropped_records": count}}).to_string();
            if let Some(output) = &mut file {
                let _ = writeln!(output, "{line}");
            } else {
                eprintln!("{line}");
            }
            pending += 1;
        }
        match message {
            Ok(WriterMessage::Record(line, urgent)) => {
                if let Some(output) = &mut file {
                    if writeln!(output, "{line}").is_err() {
                        eprintln!("{line}");
                    }
                    pending += 1;
                    if urgent || pending >= 64 {
                        let _ = output.flush();
                        pending = 0;
                    }
                } else {
                    eprintln!("{line}");
                }
            }
            Ok(WriterMessage::Shutdown(done)) => {
                if let Some(output) = &mut file {
                    let _ = output.flush();
                }
                let _ = done.send(());
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if pending > 0 {
                    if let Some(output) = &mut file {
                        let _ = output.flush();
                    }
                    pending = 0;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

/// 获取全局日志句柄；尚未 init（典型如单元测试）时返回 None。
pub fn global() -> Option<&'static Logger> {
    LOGGER.get()
}

/// 把 panic 记为 CRITICAL 日志后链回原钩子：崩溃现场可查，且不影响默认终止行为。
fn install_panic_hook() {
    let _ = PANIC_HOOK.get_or_init(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let location = info
                .location()
                .map(|value| format!("{}:{}:{}", value.file(), value.line(), value.column()))
                .unwrap_or_else(|| "unknown".into());
            let payload = info
                .payload()
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
                .unwrap_or("non-string panic payload");
            if let Some(logger) = global() {
                logger.event(
                    "CRITICAL",
                    "rust.panic",
                    payload,
                    Some("panic"),
                    None,
                    EventDetails::new(None, Some(serde_json::json!({ "location": location }))),
                )
            }
            previous(info);
        }));
    });
}

/// 文本形式的凭据脱敏：定位敏感键（token、password、authorization 等，不区分大小写），
/// 把其后 `=` 或 `:` 引出的值替换为 `<redacted>`。
///
/// 带引号的值以配对引号为界，裸值以空白或 `&`、`,`、`}` 等常见分隔符为界；
/// 这是尽力而为的兜底，不承诺覆盖所有泄漏形态。
pub fn sanitize(input: &str) -> String {
    let mut out = input.to_string();
    for key in [
        "token",
        "password",
        "passwd",
        "client_secret",
        "authorization",
        "authorization_code",
        "auth_code",
        "refresh_token",
        "access_token",
        "cookie",
        "set-cookie",
    ] {
        let mut search_from = 0;
        loop {
            let lower = out.to_ascii_lowercase();
            let Some(relative) = lower[search_from..].find(key) else {
                break;
            };
            let start = search_from + relative;
            let after_key = start + key.len();
            let Some(separator_offset) = out[after_key..].find(['=', ':']) else {
                search_from = after_key;
                continue;
            };
            let separator = after_key + separator_offset;
            let mut from = separator + 1;
            while from < out.len() && out[from..].chars().next().is_some_and(char::is_whitespace) {
                from += 1;
            }
            let quoted = out[from..].starts_with(['"', '\'']);
            if quoted {
                from += 1;
            }
            let end = if quoted {
                out[from..]
                    .find(['"', '\''])
                    .map_or(out.len(), |i| from + i)
            } else {
                out[from..]
                    .find([' ', '&', ',', ';', '\n', '\r', '}', ']'])
                    .map_or(out.len(), |i| from + i)
            };
            out.replace_range(from..end, "<redacted>");
            search_from = from + "<redacted>".len();
        }
    }
    out
}

impl Logger {
    /// 记录一条不带附加字段与 `request_id` 的日志。
    pub fn log(&self, level: &str, target: &str, message: impl AsRef<str>) {
        self.event(level, target, message, None, None, EventDetails::default());
    }

    /// 所有日志的统一出口：组装记录、脱敏、序列化后经有界通道发给写盘线程。
    ///
    /// 队列已满时 ERROR/CRITICAL 改走应急文件、普通日志仅累加丢弃计数；
    /// 通道已断（写盘线程退出）则一律走应急文件，保证退出阶段的错误不丢。
    fn event(
        &self,
        level: &str,
        target: &str,
        message: impl AsRef<str>,
        event: Option<&str>,
        request_id: Option<&str>,
        details: EventDetails,
    ) {
        let mut record_fields = details.fields.unwrap_or_else(|| serde_json::json!({}));

        // 添加 duration_ms 到 fields 中（如果存在）
        if let Some(duration) = details.duration_ms
            && let Some(obj) = record_fields.as_object_mut()
        {
            obj.insert("duration_ms".to_string(), serde_json::json!(duration));
        }

        let record = LogRecord {
            timestamp: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            level: level.to_ascii_uppercase(),
            target: target.to_string(),
            message: sanitize(message.as_ref()),
            event: event.map(str::to_string),
            request_id: request_id.map(str::to_string),
            fields: Some(sanitize_json(record_fields)),
        };
        let line = serde_json::to_string(&record).unwrap_or_else(|error| {
            format!("{{\"level\":\"ERROR\",\"message\":\"log serialization failed: {error}\"}}")
        });
        let urgent = matches!(record.level.as_str(), "ERROR" | "CRITICAL");
        match self.tx.try_send(WriterMessage::Record(line, urgent)) {
            Ok(()) => {}
            Err(TrySendError::Full(WriterMessage::Record(line, urgent))) => {
                if urgent {
                    self.write_fallback(&line);
                } else {
                    self.dropped.fetch_add(1, Ordering::Relaxed);
                }
            }
            Err(TrySendError::Disconnected(WriterMessage::Record(line, _))) => {
                self.write_fallback(&line)
            }
            Err(_) => {}
        }
    }

    /// 绕过队列把单行直接追加到应急文件；应急文件也不可用时退回 stderr。
    fn write_fallback(&self, line: &str) {
        use std::io::Write;
        // Windows release builds have no console: preserve errors in a separate
        // file when the bounded queue is full. This rare path never enqueues logs.
        let result = self.fallback.lock().ok().and_then(|path| {
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&*path)
                .ok()
                .and_then(|mut output| writeln!(output, "{line}").ok())
        });
        if result.is_none() {
            eprintln!("{line}");
        }
    }

    /// 创建操作 span：生成贯穿全程的 `request_id` 并先记一条开始事件。
    ///
    /// 即使调用方提前 return 忘记收尾，`LogSpan` 析构时也会补记 WARN，不会静默丢失。
    pub fn operation(&self, target: impl Into<String>, operation: impl Into<String>) -> LogSpan {
        let target = target.into();
        let operation = operation.into();
        let request_id = Uuid::new_v4().to_string();
        let start_time = Instant::now();

        // 记录操作开始
        self.event(
            "INFO",
            &target,
            format!("开始操作: {}", operation),
            Some("operation_start"),
            Some(&request_id),
            EventDetails::default(),
        );

        LogSpan {
            logger: self.clone(),
            target,
            operation,
            request_id,
            start_time,
            finished: false,
        }
    }

    /// 把 [`CommandError`] 连同错误码、来源与回溯记为 ERROR 事件。
    pub fn log_command_error(&self, target: &str, error: &CommandError) {
        self.event(
            "ERROR",
            target,
            &error.message,
            Some("error"),
            error.request_id.as_deref(),
            EventDetails::new(None, Some(serde_json::json!({ "code": error.code, "origin": error.diagnostics.origin, "technical": error.diagnostics.technical, "backtrace": error.diagnostics.backtrace }))),
        );
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }
}

/// 递归脱敏 JSON 值：字符串走 [`sanitize`]，键名命中敏感词时整个值替换为 `<redacted>`；
/// 数值、布尔等非字符串标量原样保留，避免破坏错误码等诊断字段。
fn sanitize_json(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::String(value) => serde_json::Value::String(sanitize(&value)),
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.into_iter().map(sanitize_json).collect())
        }
        serde_json::Value::Object(values) => serde_json::Value::Object(
            values
                .into_iter()
                .map(|(key, value)| {
                    let sensitive = [
                        "token",
                        "password",
                        "passwd",
                        "secret",
                        "authorization",
                        "cookie",
                    ]
                    .iter()
                    .any(|part| key.to_ascii_lowercase().contains(part));
                    (
                        key,
                        if sensitive {
                            serde_json::Value::String("<redacted>".into())
                        } else {
                            sanitize_json(value)
                        },
                    )
                })
                .collect(),
        ),
        other => other,
    }
}

/// 一次操作的跟踪句柄：以 `request_id` 串联开始、各步骤与结束事件，并自带计时起点。
///
/// `finished` 保证结束事件至多记录一次，重复调用 finish 系列方法是安全的。
pub struct LogSpan {
    logger: Logger,
    target: String,
    operation: String,
    request_id: String,
    start_time: Instant,
    finished: bool,
}

impl LogSpan {
    /// 记录操作成功完成，可选附加结构化字段
    pub fn finish_ok(&mut self, fields: Option<serde_json::Value>) {
        if self.finished {
            return;
        }
        self.finished = true;
        let duration_ms = self.start_time.elapsed().as_millis();
        self.logger.event(
            "INFO",
            &self.target,
            format!("操作完成: {}", self.operation),
            Some("operation_complete"),
            Some(&self.request_id),
            EventDetails::new(Some(duration_ms), fields),
        );
    }

    /// 记录操作失败
    /// 结束事件优先沿用错误自带的 `request_id`，便于与命令入口的记录对齐。
    pub fn finish_error(&mut self, error: &CommandError) {
        if self.finished {
            return;
        }
        self.finished = true;
        let duration_ms = self.start_time.elapsed().as_millis();
        self.logger.event(
            "ERROR",
            &self.target,
            format!("操作失败: {} - {}", self.operation, error.message),
            Some("operation_error"),
            error.request_id.as_deref().or(Some(&self.request_id)),
            EventDetails::new(
                Some(duration_ms),
                Some(serde_json::json!({
                    "operation": self.operation,
                    "code": error.code,
                    "origin": error.diagnostics.origin,
                    "technical": error.diagnostics.technical,
                })),
            ),
        );
    }

    /// 记录中间步骤信息
    pub fn info(&self, message: impl AsRef<str>, fields: Option<serde_json::Value>) {
        let duration_ms = self.start_time.elapsed().as_millis();
        self.logger.event(
            "INFO",
            &self.target,
            message,
            Some("step"),
            Some(&self.request_id),
            EventDetails::new(Some(duration_ms), fields),
        );
    }

    /// 记录警告信息
    pub fn warn(&self, message: impl AsRef<str>, fields: Option<serde_json::Value>) {
        let duration_ms = self.start_time.elapsed().as_millis();
        self.logger.event(
            "WARN",
            &self.target,
            message,
            Some("warning"),
            Some(&self.request_id),
            EventDetails::new(Some(duration_ms), fields),
        );
    }

    /// 记录 IO 操作（文件/网络）
    /// 成功记 DEBUG、失败记 ERROR；返回 `&Self` 支持链式调用。
    pub fn io<T, E>(&self, action: &str, result: &Result<T, E>) -> &Self
    where
        E: std::fmt::Display,
    {
        let duration_ms = self.start_time.elapsed().as_millis();
        match result {
            Ok(_) => {
                self.logger.event(
                    "DEBUG",
                    &self.target,
                    format!("IO 操作成功: {}", action),
                    Some("io_success"),
                    Some(&self.request_id),
                    EventDetails::new(
                        Some(duration_ms),
                        Some(serde_json::json!({ "action": action })),
                    ),
                );
            }
            Err(e) => {
                self.logger.event(
                    "ERROR",
                    &self.target,
                    format!("IO 操作失败: {} - {}", action, e),
                    Some("io_error"),
                    Some(&self.request_id),
                    EventDetails::new(
                        Some(duration_ms),
                        Some(serde_json::json!({ "action": action, "error": e.to_string() })),
                    ),
                );
            }
        }
        self
    }

    /// 记录文件系统操作
    pub fn fs_op<T, E>(&self, operation: &str, path: &Path, result: &Result<T, E>) -> &Self
    where
        E: std::fmt::Display,
    {
        let duration_ms = self.start_time.elapsed().as_millis();
        let path_str = path.to_string_lossy();
        match result {
            Ok(_) => {
                self.logger.event(
                    "DEBUG",
                    &self.target,
                    format!("文件操作成功: {} - {}", operation, path_str),
                    Some("fs_success"),
                    Some(&self.request_id),
                    EventDetails::new(
                        Some(duration_ms),
                        Some(serde_json::json!({ "operation": operation, "path": path_str })),
                    ),
                );
            }
            Err(e) => {
                self.logger.event(
                    "ERROR",
                    &self.target,
                    format!("文件操作失败: {} - {} - {}", operation, path_str, e),
                    Some("fs_error"),
                    Some(&self.request_id),
                    EventDetails::new(
                        Some(duration_ms),
                        Some(serde_json::json!({
                            "operation": operation,
                            "path": path_str,
                            "error": e.to_string()
                        })),
                    ),
                );
            }
        }
        self
    }

    /// 记录网络请求
    /// 状态码 >= 400 记 WARN，其余（含无状态码）记 DEBUG。
    pub fn http_request(&self, method: &str, url: &str, status: Option<u16>) {
        let duration_ms = self.start_time.elapsed().as_millis();
        let level = if status.is_some_and(|s| s >= 400) {
            "WARN"
        } else {
            "DEBUG"
        };
        self.logger.event(
            level,
            &self.target,
            format!("{} {} - {:?}", method, url, status),
            Some("http_request"),
            Some(&self.request_id),
            EventDetails::new(
                Some(duration_ms),
                Some(serde_json::json!({
                    "method": method,
                    "url": url,
                    "status": status,
                })),
            ),
        );
    }
}

impl Drop for LogSpan {
    // 兜底：未显式 finish 就被丢弃时补记 WARN，暴露“只有开始没有下文”的操作。
    fn drop(&mut self) {
        if !self.finished {
            self.finished = true;
            let duration_ms = self.start_time.elapsed().as_millis();
            self.logger.event(
                "WARN",
                &self.target,
                format!("操作未显式完成: {}", self.operation),
                Some("operation_drop"),
                Some(&self.request_id),
                EventDetails::new(Some(duration_ms), None),
            );
        }
    }
}

/// 函数版错误日志：与 `log_error!` 宏用途相同，适合不便于宏展开的调用点。
pub fn log_error(target: &str, error: &CommandError) {
    if let Some(logger) = global() {
        logger.log_command_error(target, error);
    }
}

/// 轮转策略：按修改时间只保留最近 5 个 `.jsonl`/`.txt`，更早的删除；个别失败静默忽略。
pub fn prune(directory: &Path) {
    let mut files = fs::read_dir(directory)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|x| x == "jsonl" || x == "txt")
        })
        .filter_map(|e| e.metadata().ok().map(|m| (e.path(), m.modified().ok())))
        .collect::<Vec<_>>();
    files.sort_by_key(|(_, modified)| *modified);
    while files.len() > 5 {
        if let Some((path, _)) = files.first().cloned() {
            let _ = fs::remove_file(path);
            files.remove(0);
        }
    }
}

/// 枚举目录内日志文件并按创建时间倒序返回，供前端日志面板展示。
pub fn list(directory: &Path) -> Vec<LogFileInfo> {
    let mut result = fs::read_dir(directory)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|e| {
            let path = e.path();
            let meta = e.metadata().ok()?;
            if !matches!(
                path.extension().and_then(|x| x.to_str()),
                Some("jsonl" | "txt")
            ) {
                return None;
            }
            let name = path.file_name()?.to_string_lossy().into_owned();
            Some(LogFileInfo {
                name,
                size_bytes: meta.len(),
                created_at: meta
                    .created()
                    .or_else(|_| meta.modified())
                    .ok()
                    .map(|t| chrono::DateTime::<Utc>::from(t).to_rfc3339())
                    .unwrap_or_default(),
            })
        })
        .collect::<Vec<_>>();
    result.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    result
}

/// 单个日志文件的展示信息：文件名、大小与创建时间。
#[derive(serde::Serialize, Clone)]
pub struct LogFileInfo {
    pub name: String,
    pub size_bytes: u64,
    pub created_at: String,
}

/// 返回日志目录的绝对路径。
#[tauri::command]
pub fn get_log_directory(logger: State<'_, Logger>) -> String {
    logger.directory().to_string_lossy().into_owned()
}
/// 列出全部日志文件的元信息。
#[tauri::command]
pub fn list_log_files(logger: State<'_, Logger>) -> Vec<LogFileInfo> {
    list(logger.directory())
}
/// 用系统默认程序打开日志目录。
#[tauri::command]
pub fn open_log_directory(app: AppHandle, logger: State<'_, Logger>) -> CommandResult<()> {
    app.opener()
        .open_path(logger.directory().to_string_lossy(), None::<&str>)
        .map_err(|e| CommandError::from_error("LOG_OPEN_FAILED", e))
}
/// 打开指定日志文件；先校验名字不含路径分量、扩展名合法且确实存在，防止路径穿越。
#[tauri::command]
pub fn open_log_file(app: AppHandle, logger: State<'_, Logger>, name: String) -> CommandResult<()> {
    let path = logger.directory().join(&name);
    if Path::new(&name).file_name().and_then(|n| n.to_str()) != Some(name.as_str())
        || !matches!(
            path.extension().and_then(|x| x.to_str()),
            Some("jsonl" | "txt")
        )
        || !path.is_file()
    {
        return Err(CommandError::new("INVALID_LOG_FILE", "日志文件无效"));
    }
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| CommandError::from_error("LOG_OPEN_FAILED", e))
}
/// 接收前端日志，经统一脱敏管道落盘。
#[tauri::command]
pub fn write_client_log(
    logger: State<'_, Logger>,
    level: String,
    target: String,
    message: String,
) -> CommandResult<()> {
    logger.log(&level, &target, message);
    Ok(())
}

/// 记录简单的信息日志
#[macro_export]
macro_rules! log_info {
    ($target:expr, $($arg:tt)*) => {
        if let Some(logger) = $crate::infrastructure::logging::global() {
            logger.log("INFO", $target, format!($($arg)*));
        }
    };
}

/// 记录警告日志
#[macro_export]
macro_rules! log_warn {
    ($target:expr, $($arg:tt)*) => {
        if let Some(logger) = $crate::infrastructure::logging::global() {
            logger.log("WARN", $target, format!($($arg)*));
        }
    };
}

/// 记录错误日志
#[macro_export]
macro_rules! log_error {
    ($target:expr, $($arg:tt)*) => {
        if let Some(logger) = $crate::infrastructure::logging::global() {
            logger.log("ERROR", $target, format!($($arg)*));
        }
    };
}

/// 记录调试日志
#[macro_export]
macro_rules! log_debug {
    ($target:expr, $($arg:tt)*) => {
        if let Some(logger) = $crate::infrastructure::logging::global() {
            logger.log("DEBUG", $target, format!($($arg)*));
        }
    };
}

/// 创建一个日志 span 用于跟踪操作
///
/// # Example
/// ```rust,ignore
/// let mut span = log_operation!("my_module", "process_data");
/// // ... 执行操作
/// span.finish_ok(None);
/// ```
#[macro_export]
macro_rules! log_operation {
    ($target:expr, $operation:expr) => {
        $crate::infrastructure::logging::global()
            .map(|logger| logger.operation($target, $operation))
    };
}

/// 包装一个可能失败的操作，自动记录结果
///
/// # Example
/// ```rust,ignore
/// let result = log_result!(span, "read_file", std::fs::read_to_string(&path));
/// ```
#[macro_export]
macro_rules! log_result {
    ($span:expr, $action:expr, $result:expr) => {{
        let result = $result;
        if let Some(span) = $span.as_ref() {
            span.io($action, &result);
        }
        result
    }};
}

/// 用于简化 span 完成的辅助函数
pub fn finish_span_ok<T>(mut span: Option<LogSpan>, result: T) -> T {
    if let Some(s) = span.as_mut() {
        s.finish_ok(None);
    }
    result
}

/// 按结果自动选择 `finish_ok` 或 `finish_error`，并把原结果透传给调用方。
pub fn finish_span<T>(
    mut span: Option<LogSpan>,
    result: Result<T, CommandError>,
) -> Result<T, CommandError> {
    match result {
        Ok(value) => {
            if let Some(s) = span.as_mut() {
                s.finish_ok(None);
            }
            Ok(value)
        }
        Err(error) => {
            if let Some(s) = span.as_mut() {
                s.finish_error(&error);
            }
            Err(error)
        }
    }
}

/// Called after business workers finish their shutdown logging. FIFO drains all queued records.
pub(crate) fn shutdown() {
    if let Some(logger) = global() {
        let (done, wait) = mpsc::channel();
        if logger.tx.send(WriterMessage::Shutdown(done)).is_ok() {
            let _ = wait.recv();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 验证键值对文本中的凭据会被替换为 <redacted>。
    #[test]
    fn redacts_credentials() {
        let value = sanitize("token=secret password=hunter2&ok=1");
        assert!(!value.contains("secret"));
        assert!(!value.contains("hunter2"));
        assert!(value.contains("<redacted>"));
    }

    // 验证 JSON 递归脱敏只命中敏感键，不影响错误码等诊断字段。
    #[test]
    fn redacts_nested_fields_but_keeps_diagnostic_codes() {
        let value = sanitize_json(serde_json::json!({
            "code": "NETWORK_ERROR",
            "access_token": "secret",
            "nested": { "password": "hunter2" },
        }));
        assert_eq!(value["code"], "NETWORK_ERROR");
        assert_eq!(value["access_token"], "<redacted>");
        assert_eq!(value["nested"]["password"], "<redacted>");
    }
}

#[cfg(test)]
mod writer_tests {
    use super::*;
    // 队列满时普通日志计数丢弃，错误日志落应急文件且内容已脱敏。
    #[test]
    fn overloaded_error_logs_have_a_sanitized_file_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("emergency.jsonl");
        let (tx, _rx) = mpsc::sync_channel(1);
        let logger = Logger {
            tx,
            directory: dir.path().to_path_buf(),
            dropped: Arc::new(AtomicU64::new(0)),
            fallback: Arc::new(Mutex::new(path.clone())),
        };
        logger.log("INFO", "test", "fills queue");
        logger.log("INFO", "test", "aggregated");
        logger.log("ERROR", "test", "token=private-value");
        assert_eq!(logger.dropped.load(Ordering::Relaxed), 1);
        let content = fs::read_to_string(path).unwrap();
        assert!(!content.contains("private-value"));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(content.trim()).unwrap()["level"],
            "ERROR"
        );
    }

    // 关闭时先补报合并的溢出计数，再按 FIFO 排空全部已入队记录。
    #[test]
    fn shutdown_drains_records_and_reports_aggregated_overflow() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("log.jsonl");
        let output = path.clone();
        let (tx, rx) = mpsc::sync_channel(8);
        let lost = Arc::new(AtomicU64::new(17));
        let worker = thread::spawn(move || write_records(output, rx, lost));
        for index in 0..100 {
            tx.send(WriterMessage::Record(
                serde_json::json!({"index": index}).to_string(),
                false,
            ))
            .unwrap();
        }
        let (done, wait) = mpsc::channel();
        tx.send(WriterMessage::Shutdown(done)).unwrap();
        wait.recv().unwrap();
        worker.join().unwrap();
        let text = fs::read_to_string(path).unwrap();
        let records = text
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(records.len(), 101);
        assert_eq!(records[0]["fields"]["dropped_records"], 17);
        assert_eq!(records.last().unwrap()["index"], 99);
    }
}
