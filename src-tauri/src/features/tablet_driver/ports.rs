//! Capability boundary for future OTD IPC/CLI adapters.

use crate::error::CommandResult;
use super::models::OtdStatus;

/// OTD 后端能力接口：当前仅状态读取，为将来接入 IPC/CLI 适配器预留统一入口。
pub trait OtdBackend {
    /// 读取当前 OTD 状态。
    fn status(&self) -> CommandResult<OtdStatus>;
}
