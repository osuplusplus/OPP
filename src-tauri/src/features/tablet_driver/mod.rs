//! OpenTabletDriver（OTD）数位板驱动的检测、托管启停与配置备份。
mod models;
mod service;

pub use models::OtdStatus;
pub use service::OtdRuntime;

pub mod commands;
