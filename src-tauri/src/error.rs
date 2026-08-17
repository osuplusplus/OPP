use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Error)]
#[error("{message}")]
pub struct CommandError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

impl CommandError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retry_after_seconds: None,
            request_id: None,
        }
    }

    pub fn retry_after(mut self, seconds: Option<u64>) -> Self {
        self.retry_after_seconds = seconds;
        self
    }

    pub fn request_id(mut self, request_id: Option<String>) -> Self {
        self.request_id = request_id;
        self
    }

    pub fn credentials_required() -> Self {
        Self::new("CREDENTIALS_REQUIRED", "请先配置 osu! OAuth 凭据")
    }

    pub fn auth_required() -> Self {
        Self::new("AUTH_REQUIRED", "授权已失效，请重新连接 osu! 账号")
    }

    pub fn network(message: impl Into<String>) -> Self {
        Self::new("NETWORK_ERROR", message)
    }
}

pub type CommandResult<T> = Result<T, CommandError>;

impl From<std::io::Error> for CommandError {
    fn from(error: std::io::Error) -> Self {
        Self::new("IO_ERROR", error.to_string())
    }
}

impl From<serde_json::Error> for CommandError {
    fn from(error: serde_json::Error) -> Self {
        Self::new("INVALID_DATA", error.to_string())
    }
}
