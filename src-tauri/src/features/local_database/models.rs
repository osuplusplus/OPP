//! 本地数据库面向前端的状态模型。
use serde::Serialize;

use crate::error::CommandError;

/// 数据库当前所处阶段，决定前端展示初始化、就绪还是错误界面。
#[derive(Clone, Copy, Serialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DatabasePhase {
    /// 尚未初始化，可执行首次配置。
    Unconfigured,
    /// 已成功打开，可正常使用。
    Ready,
    /// 加载失败（配置损坏、目录丢失、文件被锁等），可重试或重新定位。
    Error,
}

/// 数据库状态快照，是本 feature 全部命令的返回载体。
#[derive(Clone, Debug, Serialize)]
pub(crate) struct LocalDatabaseStatus {
    pub(crate) phase: DatabasePhase,
    /// 已配置的库目录；尚未配置时为 `None`。
    pub(crate) directory: Option<String>,
    /// 推荐目录（应用数据目录下的 `datasets`），前端可将其作为默认选项。
    pub(crate) recommended_directory: String,
    /// 数据库身份 UUID；只要配置存在即有值。
    pub(crate) database_uuid: Option<String>,
    /// 数据库 schema 版本，仅在成功打开后提供。
    pub(crate) schema_version: Option<u32>,
    /// 仅当完全没有配置且配置文件未损坏时，才允许执行首次初始化。
    pub(crate) can_initialize: bool,
    pub(crate) error: Option<CommandError>,
}
