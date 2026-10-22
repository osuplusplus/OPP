//! OTD 检测结果与配置摘要的展示模型。
use serde::Serialize;

/// OTD 配置文件关键字段的摘要。
#[derive(Debug, Clone, Serialize)]
pub struct OtdConfigSummary {
    pub output_mode: Option<String>,
    /// `Area` 节点的原始 JSON 字符串，保留细节供前端展示。
    pub area: Option<String>,
    pub filter_count: usize,
}

/// OTD 检测结果，供设置页展示。
#[derive(Debug, Clone, Serialize)]
pub struct OtdStatus {
    pub installed: bool,
    pub executable_path: Option<String>,
    pub daemon_running: bool,
    /// 仅指由 OPP 作为子进程托管的实例；外部启动的为 false。
    pub owned_by_opp: bool,
    /// 预留字段，当前实现恒为 `None`。
    pub version: Option<String>,
    pub tablet_name: Option<String>,
    pub config_path: Option<String>,
    pub config_summary: Option<OtdConfigSummary>,
    pub last_error: Option<String>,
}
