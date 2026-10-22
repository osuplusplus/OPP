//! 在线谱面模块的前后端数据契约：搜索筛选、下载请求、结果汇总与进度事件。

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::Ruleset;

/// 谱面搜索筛选条件，与前端搜索表单一一对应。
/// 结构带 `serde(default)`，前端可只传需要修改的字段；
/// 文本筛选空串表示未启用，数值范围用 `Option` 表示边界是否设置。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct OnlineBeatmapSearchQuery {
    pub query: String,
    pub ruleset: Option<Ruleset>,
    pub status: String,
    pub genre: Option<u8>,
    pub language: Option<u8>,
    pub extras: Vec<String>,
    pub include_nsfw: bool,
    pub sort: String,
    pub artist: String,
    pub title: String,
    pub title_unicode: String,
    pub source: String,
    pub mapper: String,
    pub difficulty: String,
    pub tags: String,
    pub ranked_from: String,
    pub ranked_to: String,
    pub submitted_from: String,
    pub submitted_to: String,
    pub updated_from: String,
    pub updated_to: String,
    pub favourites_min: Option<f64>,
    pub favourites_max: Option<f64>,
    pub stars_min: Option<f64>,
    pub stars_max: Option<f64>,
    pub bpm_min: Option<f64>,
    pub bpm_max: Option<f64>,
    pub length_min: Option<f64>,
    pub length_max: Option<f64>,
    pub ar_min: Option<f64>,
    pub ar_max: Option<f64>,
    pub cs_min: Option<f64>,
    pub cs_max: Option<f64>,
    pub od_min: Option<f64>,
    pub od_max: Option<f64>,
    pub hp_min: Option<f64>,
    pub hp_max: Option<f64>,
    pub keys_min: Option<f64>,
    pub keys_max: Option<f64>,
    pub cursor_string: Option<String>,
    #[serde(default)]
    pub content_filter: String,
    #[serde(default)]
    pub grade: String,
    #[serde(default)]
    pub played: String,
}

/// 多页收集后的聚合结果：`items` 已按谱面集 ID 去重，
/// `truncated` 表示因达到数量上限未能取尽官方 `available_total`。
#[derive(Debug, Serialize)]
pub struct CollectedBeatmapsets {
    pub items: Vec<Value>,
    pub available_total: Option<u64>,
    pub truncated: bool,
}

/// 单个谱面集的下载目标；`expected_beatmap_ids` 用于校验镜像压缩包难度是否与官网一致。
#[derive(Debug, Clone, Deserialize)]
pub struct BeatmapDownloadItem {
    pub beatmapset_id: u64,
    pub artist: String,
    pub title: String,
    #[serde(default)]
    pub expected_beatmap_ids: Vec<u64>,
    /// 谱面池下载允许包内包含多余难度；普通搜索仍要求难度集合精确匹配。
    /// Tournament pools select a subset of a set; ordinary searches still require an exact match.
    #[serde(default)]
    pub allow_extra_difficulties: bool,
}

/// 一次批量下载请求；`provider` 为首选镜像，失败时按内置级联顺序切换备用源。
#[derive(Debug, Deserialize)]
pub struct BeatmapDownloadRequest {
    pub destination: String,
    pub items: Vec<BeatmapDownloadItem>,
    #[serde(default = "default_download_provider")]
    pub provider: String,
    #[serde(default)]
    pub overwrite: bool,
    #[serde(default = "default_include_video")]
    pub include_video: bool,
    /// 未显式指定时回落到用户设置 `open_downloaded_beatmaps_after_download`。
    #[serde(default)]
    pub open_after_download: Option<bool>,
}

/// `provider` 字段缺省时回落到小夜镜像，与下载级联的首选一致。
fn default_download_provider() -> String {
    "sayobot".into()
}

/// `include_video` 缺省时下载带视频的完整曲包。
fn default_include_video() -> bool {
    true
}

/// 单个谱面集的失败信息；`message` 汇总了所有尝试过的镜像的失败原因。
#[derive(Debug, Clone, Serialize)]
pub struct BeatmapDownloadFailure {
    pub beatmapset_id: u64,
    pub title: String,
    pub message: String,
}

/// 批量下载的最终汇总；`succeeded_beatmapset_ids` 同时包含新下载成功与跳过的谱面集。
#[derive(Debug, Serialize)]
pub struct BeatmapDownloadResult {
    pub destination: String,
    pub total: usize,
    pub completed: usize,
    pub skipped: usize,
    pub failed: usize,
    /// 置位为 true 表示任务被取消；被取消的项目不计入失败数。
    pub cancelled: bool,
    pub failures: Vec<BeatmapDownloadFailure>,
    pub completed_paths: Vec<String>,
    pub succeeded_beatmapset_ids: Vec<u64>,
}

/// 经 `beatmap-download-progress` 事件推送的进度快照；
/// `completed_paths` 与 `destination` 仅在结束事件中填充。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BeatmapDownloadProgress {
    pub phase: String,
    pub total: usize,
    pub processed: usize,
    pub completed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub current_beatmapset_id: Option<u64>,
    pub current_title: Option<String>,
    pub message: Option<String>,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub bytes_per_second: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_paths: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination: Option<String>,
}

/// 批量任务的实时计数，由并发下载任务共享并用于构造进度事件。
#[derive(Clone)]
pub struct DownloadProgressCounts {
    pub total: usize,
    pub processed: usize,
    pub completed: usize,
    pub skipped: usize,
    pub failed: usize,
}
