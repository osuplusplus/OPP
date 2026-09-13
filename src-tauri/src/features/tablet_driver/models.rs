use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct OtdConfigSummary {
    pub output_mode: Option<String>,
    pub area: Option<String>,
    pub filter_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct OtdStatus {
    pub installed: bool,
    pub executable_path: Option<String>,
    pub daemon_running: bool,
    pub owned_by_opp: bool,
    pub version: Option<String>,
    pub tablet_name: Option<String>,
    pub config_path: Option<String>,
    pub config_summary: Option<OtdConfigSummary>,
    pub last_error: Option<String>,
}
