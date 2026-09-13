//! Capability boundary for future OTD IPC/CLI adapters.

use crate::error::CommandResult;
use super::models::OtdStatus;

pub trait OtdBackend {
    fn status(&self) -> CommandResult<OtdStatus>;
}
