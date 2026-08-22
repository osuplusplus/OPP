use crate::app::default::{
    default_cache_limit_mb, default_danser_encoder, default_danser_fps,
    default_danser_frame_height, default_danser_frame_width, default_danser_motion_blur_oversample,
    default_danser_quality, default_danser_settings_profile, default_launch_tosu_lyrics,
    default_obs_websocket_url, default_one, default_open_local_maps_key, default_open_settings_key,
    default_open_trainer_key, default_preview_volume, default_similarity_results_per_page,
    default_similarity_section_range, default_theme_mode, default_theme_primary,
    default_theme_secondary, default_tosu_api_base_url, default_true,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{collections::BTreeMap, fmt};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Ruleset {
    Osu,
    Taiko,
    Fruits,
    Mania,
}

impl fmt::Display for Ruleset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Osu => "osu",
            Self::Taiko => "taiko",
            Self::Fruits => "fruits",
            Self::Mania => "mania",
        };
        f.write_str(value)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ScoreCategory {
    Best,
    Pinned,
    Recent,
}

impl fmt::Display for ScoreCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Best => "best",
            Self::Pinned => "pinned",
            Self::Recent => "recent",
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnProfile {
    pub id: u64,
    pub username: String,
    pub avatar_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_data_url: Option<String>,
    pub country_code: String,
    #[serde(default)]
    pub is_active: bool,
    #[serde(default)]
    pub is_online: bool,
    #[serde(default)]
    pub is_supporter: bool,
    #[serde(default)]
    pub is_restricted: Option<bool>,
    #[serde(default)]
    pub last_visit: Option<String>,
    #[serde(default)]
    pub playmode: Option<String>,
    #[serde(default)]
    pub statistics: Option<Value>,
    #[serde(default)]
    pub statistics_rulesets: Option<Value>,
    #[serde(default)]
    pub rank_history: Option<Value>,
    #[serde(default)]
    pub monthly_playcounts: Option<Vec<Value>>,
    #[serde(default)]
    pub replays_watched_counts: Option<Vec<Value>>,
    #[serde(default)]
    pub badges: Option<Vec<Value>>,
    #[serde(default)]
    pub groups: Option<Vec<Value>>,
    #[serde(default)]
    pub user_achievements: Option<Vec<Value>>,
    #[serde(default)]
    pub account_history: Option<Vec<Value>>,
    #[serde(default)]
    pub page: Option<Value>,
    #[serde(default)]
    pub cover: Option<Value>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Score {
    #[serde(default)]
    pub id: Option<u64>,
    pub user_id: u64,
    #[serde(default)]
    pub accuracy: f64,
    #[serde(default)]
    pub pp: Option<f64>,
    #[serde(default)]
    pub rank: String,
    #[serde(default)]
    pub total_score: Option<u64>,
    #[serde(default)]
    pub legacy_total_score: Option<u64>,
    #[serde(default)]
    pub max_combo: Option<u64>,
    #[serde(default)]
    pub ended_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub has_replay: Option<bool>,
    #[serde(default)]
    pub mods: Vec<Value>,
    #[serde(default)]
    pub statistics: Value,
    #[serde(default)]
    pub maximum_statistics: Option<Value>,
    #[serde(default)]
    pub beatmap: Option<Value>,
    #[serde(default)]
    pub beatmapset: Option<Value>,
    #[serde(default)]
    pub weight: Option<Value>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cached<T> {
    pub data: T,
    pub fetched_at: DateTime<Utc>,
    pub stale: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default)]
    pub onboarding_version: u32,
    #[serde(default)]
    pub page_onboarding_versions: BTreeMap<String, u32>,
    #[serde(default)]
    pub ignored_update_version: Option<String>,
    #[serde(default)]
    pub reduce_motion: bool,
    #[serde(default)]
    pub similarity_index_directory: Option<String>,
    #[serde(default)]
    pub mania_similarity_index_directory: Option<String>,
    #[serde(default)]
    pub similarity_preferences: SimilarityPreferences,
    #[serde(default)]
    pub beatmap_download_directory: Option<String>,
    #[serde(default)]
    pub default_beatmap_download_provider: BeatmapDownloadProvider,
    #[serde(default = "default_true")]
    pub include_video_in_beatmap_downloads: bool,
    #[serde(default)]
    pub open_downloaded_beatmaps_after_download: bool,
    #[serde(default)]
    pub replay_export_directory: Option<String>,
    #[serde(default)]
    pub danser_executable_path: Option<String>,
    /// FFmpeg 可执行文件路径
    #[serde(default)]
    pub ffmpeg_executable_path: Option<String>,
    #[serde(default)]
    pub auto_export_new_replays_with_danser: bool,
    #[serde(default)]
    pub danser_render_preferences: DanserRenderPreferences,
    #[serde(default)]
    pub tosu_executable_path: Option<String>,
    #[serde(default = "default_tosu_api_base_url")]
    pub tosu_api_base_url: String,
    #[serde(default)]
    pub launch_tosu_with_game: bool,
    #[serde(default)]
    pub tosu_lyrics_executable_path: Option<String>,
    #[serde(default = "default_launch_tosu_lyrics")]
    pub launch_tosu_lyrics_with_tosu: bool,
    #[serde(default = "default_theme_primary")]
    pub theme_primary: String,
    #[serde(default = "default_theme_secondary")]
    pub theme_secondary: String,
    #[serde(default = "default_theme_mode")]
    pub theme_mode: String,
    #[serde(default)]
    pub launch_tosu_on_game_detect: bool,
    #[serde(default = "default_obs_websocket_url")]
    pub obs_websocket_url: String,
    #[serde(default)]
    pub obs_selected_scene: Option<String>,
    #[serde(default)]
    pub launch_tosu_on_obs_detect: bool,
    #[serde(default)]
    pub suppress_tosu_launch_prompt: bool,
    #[serde(default)]
    pub game_session_analysis_on_detect: bool,
    #[serde(default = "default_preview_volume")]
    pub preview_volume: u8,
    /// Maximum disk space reserved for generated local-analysis thumbnails.
    #[serde(default = "default_cache_limit_mb")]
    pub cache_limit_mb: u32,
    #[serde(default)]
    pub key_bindings: AppKeyBindings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DanserRenderPreferences {
    #[serde(default = "default_danser_settings_profile")]
    pub settings_profile: String,
    #[serde(default)]
    pub skin: String,
    #[serde(default = "default_true")]
    pub skip: bool,
    #[serde(default)]
    pub quickstart: bool,
    #[serde(default)]
    pub start: Option<f64>,
    #[serde(default)]
    pub end: Option<f64>,
    #[serde(default = "default_one")]
    pub speed: f64,
    #[serde(default = "default_one")]
    pub pitch: f64,
    #[serde(default)]
    pub offset: i32,
    #[serde(default)]
    pub mods: String,
    #[serde(default)]
    pub mods2: String,
    #[serde(default)]
    pub cs: Option<f64>,
    #[serde(default)]
    pub ar: Option<f64>,
    #[serde(default)]
    pub od: Option<f64>,
    #[serde(default)]
    pub hp: Option<f64>,
    #[serde(default = "default_true")]
    pub no_db_check: bool,
    #[serde(default = "default_true")]
    pub no_update_check: bool,
    #[serde(default)]
    pub debug: bool,
    #[serde(default)]
    pub settings_patch: String,
    #[serde(default = "default_danser_frame_width")]
    pub frame_width: u32,
    #[serde(default = "default_danser_frame_height")]
    pub frame_height: u32,
    #[serde(default = "default_danser_fps")]
    pub fps: u32,
    #[serde(default = "default_danser_encoder")]
    pub encoder: String,
    #[serde(default = "default_danser_quality")]
    pub quality: u8,
    #[serde(default)]
    pub motion_blur: bool,
    #[serde(default = "default_danser_motion_blur_oversample")]
    pub motion_blur_oversample: u32,
}

impl Default for DanserRenderPreferences {
    fn default() -> Self {
        Self {
            settings_profile: default_danser_settings_profile(),
            skin: String::new(),
            skip: true,
            quickstart: false,
            start: None,
            end: None,
            speed: 1.0,
            pitch: 1.0,
            offset: 0,
            mods: String::new(),
            mods2: String::new(),
            cs: None,
            ar: None,
            od: None,
            hp: None,
            no_db_check: true,
            no_update_check: true,
            debug: false,
            settings_patch: String::new(),
            frame_width: default_danser_frame_width(),
            frame_height: default_danser_frame_height(),
            fps: default_danser_fps(),
            encoder: default_danser_encoder(),
            quality: default_danser_quality(),
            motion_blur: false,
            motion_blur_oversample: default_danser_motion_blur_oversample(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppKeyBindings {
    #[serde(default = "default_open_local_maps_key")]
    pub open_local_maps: String,
    #[serde(default = "default_open_trainer_key")]
    pub open_trainer: String,
    #[serde(default = "default_open_settings_key")]
    pub open_settings: String,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SimilarityWeightingPreference {
    #[default]
    Dynamic,
    Manual,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SimilarityManualWeights {
    pub aim: f32,
    pub speed: f32,
    pub reading: f32,
    pub slider: f32,
    pub overlap: f32,
    pub parameters: f32,
}

impl Default for SimilarityManualWeights {
    fn default() -> Self {
        Self {
            aim: 1.0,
            speed: 2.0,
            reading: 2.0,
            slider: 0.0,
            overlap: 0.25,
            parameters: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SimilarityPreferences {
    #[serde(default)]
    pub advanced_enabled: bool,
    #[serde(default)]
    pub mode: SimilarityWeightingPreference,
    #[serde(default = "default_similarity_section_range")]
    pub lower_sections: u32,
    #[serde(default = "default_similarity_section_range")]
    pub upper_sections: u32,
    #[serde(default)]
    pub manual_weights: SimilarityManualWeights,
    #[serde(default = "default_similarity_results_per_page")]
    pub results_per_page: u32,
}

impl Default for SimilarityPreferences {
    fn default() -> Self {
        Self {
            advanced_enabled: false,
            mode: SimilarityWeightingPreference::Manual,
            lower_sections: default_similarity_section_range(),
            upper_sections: default_similarity_section_range(),
            manual_weights: SimilarityManualWeights::default(),
            results_per_page: default_similarity_results_per_page(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BeatmapDownloadProvider {
    #[default]
    Sayobot,
    Hinai,
    Catboy,
    Nerinyan,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            onboarding_version: 0,
            page_onboarding_versions: BTreeMap::new(),
            ignored_update_version: None,
            reduce_motion: false,
            similarity_index_directory: None,
            mania_similarity_index_directory: None,
            similarity_preferences: SimilarityPreferences::default(),
            beatmap_download_directory: None,
            default_beatmap_download_provider: BeatmapDownloadProvider::default(),
            include_video_in_beatmap_downloads: true,
            open_downloaded_beatmaps_after_download: false,
            replay_export_directory: None,
            danser_executable_path: None,
            ffmpeg_executable_path: None,
            auto_export_new_replays_with_danser: false,
            danser_render_preferences: DanserRenderPreferences::default(),
            tosu_executable_path: None,
            tosu_api_base_url: default_tosu_api_base_url(),
            launch_tosu_with_game: false,
            tosu_lyrics_executable_path: None,
            launch_tosu_lyrics_with_tosu: default_launch_tosu_lyrics(),
            theme_primary: default_theme_primary(),
            theme_secondary: default_theme_secondary(),
            theme_mode: default_theme_mode(),
            launch_tosu_on_game_detect: false,
            obs_websocket_url: default_obs_websocket_url(),
            obs_selected_scene: None,
            launch_tosu_on_obs_detect: false,
            suppress_tosu_launch_prompt: false,
            game_session_analysis_on_detect: false,
            preview_volume: default_preview_volume(),
            cache_limit_mb: default_cache_limit_mb(),
            key_bindings: AppKeyBindings::default(),
        }
    }
}

impl Default for AppKeyBindings {
    fn default() -> Self {
        Self {
            open_local_maps: default_open_local_maps_key(),
            open_trainer: default_open_trainer_key(),
            open_settings: default_open_settings_key(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthStatus {
    pub credentials_configured: bool,
    pub connected: bool,
    pub client_id: Option<String>,
    pub callback_url: String,
    pub user_id: Option<u64>,
    pub username: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingOAuth {
    pub authorization_url: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthResult {
    pub ok: bool,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedCredentials {
    pub client_id: String,
    pub callback_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisconnectResult {
    pub revoked: bool,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheRecord {
    pub value: Value,
    pub fetched_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PersistedState {
    pub client_id: Option<String>,
    pub token_expires_at: Option<DateTime<Utc>>,
    pub current_user_id: Option<u64>,
    pub username: Option<String>,
    #[serde(default)]
    pub settings: AppSettings,
    #[serde(default)]
    pub cache: BTreeMap<String, CacheRecord>,
    #[serde(default)]
    pub last_manual_refresh: BTreeMap<String, DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    pub expires_in: i64,
}
