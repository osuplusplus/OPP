use chrono::Utc;
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        OnceLock,
        mpsc::{self, Sender},
    },
    thread,
    time::Instant,
};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;
use uuid::Uuid;

use crate::error::CommandError;
use crate::error::CommandResult;

#[derive(Clone)]
pub struct Logger {
    tx: Sender<String>,
    directory: PathBuf,
    current: String,
}

static LOGGER: OnceLock<Logger> = OnceLock::new();
static PANIC_HOOK: OnceLock<()> = OnceLock::new();

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
    let (tx, rx) = mpsc::channel::<String>();
    let _ = thread::Builder::new()
        .name("opp-log-writer".into())
        .spawn(move || {
            use std::io::Write;
            let mut file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .ok();
            for line in rx {
                if let Some(output) = file.as_mut() {
                    let _ = writeln!(output, "{line}");
                    let _ = output.flush();
                } else {
                    eprintln!("{line}");
                }
            }
        });
    let logger = Logger {
        tx,
        directory,
        current,
    };
    let _ = LOGGER.set(logger.clone());
    install_panic_hook();
    logger
}

pub fn global() -> Option<&'static Logger> {
    LOGGER.get()
}

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
                    None,
                    Some(serde_json::json!({ "location": location })),
                )
            }
            previous(info);
        }));
    });
}

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
            let Some(separator_offset) = out[after_key..].find(|ch: char| ch == '=' || ch == ':')
            else {
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
    pub fn log(&self, level: &str, target: &str, message: impl AsRef<str>) {
        self.event(level, target, message, None, None, None, None);
    }

    pub fn event(
        &self,
        level: &str,
        target: &str,
        message: impl AsRef<str>,
        event: Option<&str>,
        request_id: Option<&str>,
        duration_ms: Option<u128>,
        fields: Option<serde_json::Value>,
    ) {
        let mut record_fields = fields.unwrap_or_else(|| serde_json::json!({}));

        // 添加 duration_ms 到 fields 中（如果存在）
        if let Some(duration) = duration_ms {
            if let Some(obj) = record_fields.as_object_mut() {
                obj.insert("duration_ms".to_string(), serde_json::json!(duration));
            }
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
        let _ = self.tx.send(line);
    }

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
            None,
            None,
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

    pub fn log_command_error(&self, target: &str, error: &CommandError) {
        self.event(
            "ERROR",
            target,
            &error.message,
            Some("error"),
            error.request_id.as_deref(),
            None,
            Some(serde_json::json!({ "code": error.code, "origin": error.origin, "technical": error.technical, "backtrace": error.backtrace })),
        );
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub fn current_file(&self) -> PathBuf {
        self.directory.join(&self.current)
    }
}

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

pub struct LogSpan {
    logger: Logger,
    target: String,
    operation: String,
    request_id: String,
    start_time: Instant,
    finished: bool,
}

impl LogSpan {
    pub fn request_id(&self) -> &str {
        &self.request_id
    }

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
            Some(duration_ms),
            fields,
        );
    }

    /// 记录操作失败
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
            Some(duration_ms),
            Some(serde_json::json!({
                "operation": self.operation,
                "code": error.code,
                "origin": error.origin,
                "technical": error.technical,
            })),
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
            Some(duration_ms),
            fields,
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
            Some(duration_ms),
            fields,
        );
    }

    /// 记录 IO 操作（文件/网络）
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
                    Some(duration_ms),
                    Some(serde_json::json!({ "action": action })),
                );
            }
            Err(e) => {
                self.logger.event(
                    "ERROR",
                    &self.target,
                    format!("IO 操作失败: {} - {}", action, e),
                    Some("io_error"),
                    Some(&self.request_id),
                    Some(duration_ms),
                    Some(serde_json::json!({ "action": action, "error": e.to_string() })),
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
                    Some(duration_ms),
                    Some(serde_json::json!({ "operation": operation, "path": path_str })),
                );
            }
            Err(e) => {
                self.logger.event(
                    "ERROR",
                    &self.target,
                    format!("文件操作失败: {} - {} - {}", operation, path_str, e),
                    Some("fs_error"),
                    Some(&self.request_id),
                    Some(duration_ms),
                    Some(serde_json::json!({
                        "operation": operation,
                        "path": path_str,
                        "error": e.to_string()
                    })),
                );
            }
        }
        self
    }

    /// 记录网络请求
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
            Some(duration_ms),
            Some(serde_json::json!({
                "method": method,
                "url": url,
                "status": status,
            })),
        );
    }
}

impl Drop for LogSpan {
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
                Some(duration_ms),
                None,
            );
        }
    }
}

pub fn log_error(target: &str, error: &CommandError) {
    if let Some(logger) = global() {
        logger.log_command_error(target, error);
    }
}

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

#[derive(serde::Serialize, Clone)]
pub struct LogFileInfo {
    pub name: String,
    pub size_bytes: u64,
    pub created_at: String,
}

#[tauri::command]
pub fn get_log_directory(logger: State<'_, Logger>) -> String {
    logger.directory().to_string_lossy().into_owned()
}
#[tauri::command]
pub fn list_log_files(logger: State<'_, Logger>) -> Vec<LogFileInfo> {
    list(logger.directory())
}
#[tauri::command]
pub fn open_log_directory(app: AppHandle, logger: State<'_, Logger>) -> CommandResult<()> {
    app.opener()
        .open_path(logger.directory().to_string_lossy(), None::<&str>)
        .map_err(|e| CommandError::from_error("LOG_OPEN_FAILED", e))
}
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redacts_credentials() {
        let value = sanitize("token=secret password=hunter2&ok=1");
        assert!(!value.contains("secret"));
        assert!(!value.contains("hunter2"));
        assert!(value.contains("<redacted>"));
    }

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

/// 日志宏和辅助函数
///
/// 这些宏简化了常见的日志场景，自动处理 span 的创建和完成。

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

/// 辅助函数：包装 Result 并记录 IO 操作
pub fn log_io_result<T, E>(
    span: &Option<LogSpan>,
    action: &str,
    result: Result<T, E>,
) -> Result<T, E>
where
    E: std::fmt::Display,
{
    if let Some(s) = span {
        s.io(action, &result);
    }
    result
}

/// 辅助函数：包装 Result 并记录文件系统操作
pub fn log_fs_result<T, E>(
    span: &Option<LogSpan>,
    operation: &str,
    path: &Path,
    result: Result<T, E>,
) -> Result<T, E>
where
    E: std::fmt::Display,
{
    if let Some(s) = span {
        s.fs_op(operation, path, &result);
    }
    result
}

/// 用于简化 span 完成的辅助函数
pub fn finish_span_ok<T>(mut span: Option<LogSpan>, result: T) -> T {
    if let Some(s) = span.as_mut() {
        s.finish_ok(None);
    }
    result
}

pub fn finish_span_ok_with_fields<T>(
    mut span: Option<LogSpan>,
    result: T,
    fields: serde_json::Value,
) -> T {
    if let Some(s) = span.as_mut() {
        s.finish_ok(Some(fields));
    }
    result
}

pub fn finish_span_err<T>(
    mut span: Option<LogSpan>,
    error: CommandError,
) -> Result<T, CommandError> {
    if let Some(s) = span.as_mut() {
        s.finish_error(&error);
    }
    Err(error)
}

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
