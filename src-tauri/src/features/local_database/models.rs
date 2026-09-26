use serde::Serialize;

use crate::error::CommandError;

#[derive(Clone, Copy, Serialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DatabasePhase {
    Unconfigured,
    Ready,
    Error,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct LocalDatabaseStatus {
    pub(crate) phase: DatabasePhase,
    pub(crate) directory: Option<String>,
    pub(crate) recommended_directory: String,
    pub(crate) database_uuid: Option<String>,
    pub(crate) schema_version: Option<u32>,
    pub(crate) can_initialize: bool,
    pub(crate) error: Option<CommandError>,
}
