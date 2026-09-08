use serde::Serialize;
use std::{
    backtrace::{Backtrace, BacktraceStatus},
    error::Error as StdError,
    panic::Location,
};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Error)]
#[error("{message}")]
pub struct CommandError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// 面向日志的错误链摘要；不直接展示给用户，避免泄露实现细节。
    #[serde(skip)]
    pub technical: Option<String>,
    /// 仅在运行时启用回溯采集时写入日志。
    #[serde(skip)]
    pub backtrace: Option<String>,
    #[serde(skip)]
    pub origin: Option<String>,
}

impl CommandError {
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
            technical: None,
            backtrace: captured_backtrace(),
            origin: Some(format!(
                "{}:{}:{}",
                caller.file(),
                caller.line(),
                caller.column()
            )),
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
            result.technical = Some(chain.join(" -> "));
        }
        crate::infrastructure::logging::log_error("command", &result);
        result
    }

    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        let context = context.into();
        self.technical = Some(match self.technical.take() {
            Some(existing) => format!("{context}: {existing}"),
            None => context,
        });
        crate::infrastructure::logging::log_error("command", &self);
        self
    }

    pub fn retry_after(mut self, seconds: Option<u64>) -> Self {
        self.retry_after_seconds = seconds;
        self
    }

    pub fn request_id(mut self, request_id: Option<String>) -> Self {
        if request_id.is_some() {
            self.request_id = request_id;
        }
        self
    }

    #[track_caller]
    pub fn credentials_required() -> Self {
        Self::new("CREDENTIALS_REQUIRED", "请先配置 osu! OAuth 凭据")
    }

    #[track_caller]
    pub fn auth_required() -> Self {
        Self::new("AUTH_REQUIRED", "授权已失效，请重新连接 osu! 账号")
    }

    #[track_caller]
    pub fn network(message: impl Into<String>) -> Self {
        Self::new("NETWORK_ERROR", message)
    }
}

fn captured_backtrace() -> Option<String> {
    let backtrace = Backtrace::capture();
    (backtrace.status() == BacktraceStatus::Captured).then(|| backtrace.to_string())
}

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
