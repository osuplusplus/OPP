use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::Ruleset;

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

#[derive(Debug, Serialize)]
pub struct CollectedBeatmapsets {
    pub items: Vec<Value>,
    pub available_total: Option<u64>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BeatmapDownloadItem {
    pub beatmapset_id: u64,
    pub artist: String,
    pub title: String,
}

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
    #[serde(default)]
    pub open_after_download: Option<bool>,
}

fn default_download_provider() -> String {
    "sayobot".into()
}

fn default_include_video() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
pub struct BeatmapDownloadFailure {
    pub beatmapset_id: u64,
    pub title: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct BeatmapDownloadResult {
    pub destination: String,
    pub total: usize,
    pub completed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub cancelled: bool,
    pub failures: Vec<BeatmapDownloadFailure>,
    pub completed_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
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

pub struct DownloadProgressCounts {
    pub total: usize,
    pub processed: usize,
    pub completed: usize,
    pub skipped: usize,
    pub failed: usize,
}
