use serde::{Deserialize, Serialize};

use crate::domain::Ruleset;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CareerStatus {
    pub configured: bool,
    pub path: Option<String>,
    pub snapshot_count: u64,
    pub latest_date: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CareerCaptureResult {
    pub local_date: String,
    pub status: String,
    pub created: bool,
    pub fetched_at: Option<String>,
    pub profile_available: bool,
    pub scores_available: bool,
    pub media_available: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CareerStats {
    pub pp: Option<f64>,
    pub global_rank: Option<f64>,
    pub country_rank: Option<f64>,
    pub ranked_score: Option<f64>,
    pub total_score: Option<f64>,
    pub hit_accuracy: Option<f64>,
    pub play_count: Option<f64>,
    pub play_time: Option<f64>,
    pub total_hits: Option<f64>,
    pub maximum_combo: Option<f64>,
    pub level: Option<f64>,
    pub level_progress: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CareerCalendarDay {
    pub date: String,
    pub status: String,
    pub captured_at: Option<String>,
    pub stats: Option<CareerStats>,
    pub error: Option<String>,
    pub has_diff: bool,
    pub added_scores: u32,
    pub removed_scores: u32,
    pub changed_scores: u32,
    pub added_medals: u32,
    pub added_replays: u32,
    pub added_screenshots: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CareerCalendar {
    pub ruleset: Ruleset,
    pub start_date: String,
    pub end_date: String,
    pub days: Vec<CareerCalendarDay>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CareerScoreDiff {
    pub kind: String,
    pub key: String,
    pub before_position: Option<u32>,
    pub after_position: Option<u32>,
    pub score: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CareerEventItem {
    pub kind: String,
    pub client: Option<String>,
    pub name: String,
    pub path: Option<String>,
    pub size: Option<u64>,
    pub modified_at: Option<String>,
    pub payload: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CareerDayDetail {
    pub ruleset: Ruleset,
    pub date: String,
    pub status: String,
    pub captured_at: Option<String>,
    pub stats: Option<CareerStats>,
    pub previous_stats: Option<CareerStats>,
    pub error: Option<String>,
    pub score_diffs: Vec<CareerScoreDiff>,
    pub medal_events: Vec<CareerEventItem>,
    pub media_events: Vec<CareerEventItem>,
}
