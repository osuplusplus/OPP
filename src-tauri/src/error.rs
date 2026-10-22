//! 统一错误契约：所有 Tauri 命令以 `CommandResult` 返回，错误序列化后
//! 只向前端暴露 `code`/`message`/`retry_after_seconds`/`request_id`，
//! 回溯与来源等诊断信息仅写入服务端日志。
use serde::Serialize;
use std::{
    backtrace::{Backtrace, BacktraceStatus},
    error::Error as StdError,
    panic::Location,
};
use thiserror::Error;
use uuid::Uuid;

/// 命令统一错误：`code` 供前端逻辑分支，`message` 为可直接展示的用户文案；
/// 诊断载荷不参与序列化。
#[derive(Debug, Clone, Serialize, Error)]
#[error("{message}")]
pub struct CommandError {
    pub code: String,
    pub message: String,
    /// 限流场景下建议前端等待的秒数，仅部分错误携带。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_seconds: Option<u64>,
    /// 用于在日志中关联同一请求的标识。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// 仅用于日志的诊断信息，不传给前端。
    #[serde(skip)]
    pub(crate) diagnostics: Box<ErrorDiagnostics>,
}

/// 仅写入日志的诊断载荷：`technical` 为 source 错误链拼接，
/// `backtrace` 为构造时捕获的回溯，`origin` 为构造点源码位置。
#[derive(Debug, Clone)]
pub(crate) struct ErrorDiagnostics {
    pub technical: Option<String>,
    pub backtrace: Option<String>,
    pub origin: Option<String>,
}

impl From<tauri::Error> for CommandError {
    fn from(error: tauri::Error) -> Self {
        Self::from_error("WINDOW_OPERATION_ERROR", error)
    }
}

impl CommandError {
    /// 以稳定错误码与用户文案构造错误，并立即写入错误日志。
    #[track_caller]
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        let error = Self::build(code, message);
        crate::infrastructure::logging::log_error("command", &error);
        error
    }

    #[track_caller]
    fn build(code: impl Into<String>, message: impl Into<String>) -> Self {
        let caller = Location::caller();
        Self {
            code: code.into(),
            message: message.into(),
            retry_after_seconds: None,
            request_id: Some(Uuid::new_v4().to_string()),
            diagnostics: Box::new(ErrorDiagnostics {
                technical: None,
                backtrace: captured_backtrace(),
                origin: Some(format!(
                    "{}:{}:{}",
                    caller.file(),
                    caller.line(),
                    caller.column()
                )),
            }),
        }
    }

    /// 从 Rust 原生错误构造统一命令错误，同时保留 source 链和可选回溯。
    #[track_caller]
    pub fn from_error(code: impl Into<String>, error: impl StdError) -> Self {
        let mut result = Self::build(code, error.to_string());
        let mut chain = Vec::new();
        let mut source = error.source();
        while let Some(item) = source {
            chain.push(item.to_string());
            source = item.source();
        }
        if !chain.is_empty() {
            result.diagnostics.technical = Some(chain.join(" -> "));
        }
        crate::infrastructure::logging::log_error("command", &result);
        result
    }

    /// 链式附加限流重试等待秒数。
    pub fn retry_after(mut self, seconds: Option<u64>) -> Self {
        self.retry_after_seconds = seconds;
        self
    }

    /// 链式覆盖自动生成的请求 ID；传入 `None` 时保持原值。
    pub fn request_id(mut self, request_id: Option<String>) -> Self {
        if request_id.is_some() {
            self.request_id = request_id;
        }
        self
    }

    /// 未配置 osu! OAuth 凭据时的标准错误。
    #[track_caller]
    pub fn credentials_required() -> Self {
        Self::new("CREDENTIALS_REQUIRED", "请先配置 osu! OAuth 凭据")
    }

    /// 授权失效时的标准错误，前端应引导重新连接账号。
    #[track_caller]
    pub fn auth_required() -> Self {
        Self::new("AUTH_REQUIRED", "授权已失效，请重新连接 osu! 账号")
    }

    /// 网络请求失败的标准错误。
    #[track_caller]
    pub fn network(message: impl Into<String>) -> Self {
        Self::new("NETWORK_ERROR", message)
    }
}

/// 仅当进程启用回溯捕获（如设置 `RUST_BACKTRACE`）时才返回回溯文本。
fn captured_backtrace() -> Option<String> {
    let backtrace = Backtrace::capture();
    (backtrace.status() == BacktraceStatus::Captured).then(|| backtrace.to_string())
}

/// 所有 Tauri 命令的统一返回类型。
pub type CommandResult<T> = Result<T, CommandError>;

impl From<std::io::Error> for CommandError {
    fn from(error: std::io::Error) -> Self {
        Self::from_error("IO_ERROR", error)
    }
}

impl From<serde_json::Error> for CommandError {
    fn from(error: serde_json::Error) -> Self {
        Self::from_error("INVALID_DATA", error)
    }
}

#[cfg(test)]
mod tests {
    use super::CommandError;

    // 验证序列化契约：前端只收到四个公开字段，诊断载荷不外泄。
    #[test]
    fn serializes_only_frontend_error_fields() {
        let error = CommandError::new("TEST_ERROR", "test message").retry_after(Some(5));
        let value = serde_json::to_value(error).expect("command errors are serializable");

        assert_eq!(value["code"], "TEST_ERROR");
        assert_eq!(value["message"], "test message");
        assert_eq!(value["retry_after_seconds"], 5);
        assert!(value["request_id"].is_string());
        assert!(value.get("diagnostics").is_none());
    }
}
