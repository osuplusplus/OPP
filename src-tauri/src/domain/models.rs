//! 跨 feature 共享的稳定领域模型：游戏模式、成绩与账号资料、应用设置、
//! OAuth 令牌等；同时是对前端的序列化契约，字段名不可随意变更。
use super::defaults::{
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

/// osu! 四种游戏模式；serde 与 `Display` 均输出小写名称
/// （`osu`/`taiko`/`fruits`/`mania`）。
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

/// 成绩列表类别：最佳（best）、置顶（pinned）、最近（recent）。
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

/// osu! API 的当前用户资料：核心字段强类型，低频扩展字段以原始 JSON 透传，
/// 未知字段经 `extra` 平铺保留，API 新增字段时不致丢失。
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

/// osu! API 的单条成绩：统计与谱面信息以原始 JSON 透传，
/// 未知字段经 `extra` 平铺保留。
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

/// 带抓取时间的缓存包装：`stale` 为 `true` 表示已超过缓存有效期，
/// 旧值仍随响应返回供前端先行展示。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cached<T> {
    pub data: T,
    pub fetched_at: DateTime<Utc>,
    pub stale: bool,
}

/// 本地谱面存在性统计的范围：全部、仅 Stable、仅 Lazer。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LocalPresenceScope {
    #[default]
    All,
    Stable,
    Lazer,
}

/// 持久化到应用数据目录的用户设置；所有字段均带 serde 默认值，
/// 旧版本配置文件缺少新字段时可无损加载。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default = "default_true")]
    pub show_local_beatmap_presence: bool,
    #[serde(default)]
    pub local_beatmap_presence_scope: LocalPresenceScope,
    #[serde(default)]
    pub collection_manager_path: Option<String>,
    /// 集合备份的保留份数。
    #[serde(default = "default_collection_backup_retention")]
    pub collection_backup_retention: usize,
    #[serde(default)]
    pub collection_backup_directory: Option<String>,
    /// 用户已完成的全局新手引导版本，用于决定是否再次展示引导。
    #[serde(default)]
    pub onboarding_version: u32,
    /// 各页面新手引导的完成版本，键为页面标识。
    #[serde(default)]
    pub page_onboarding_versions: BTreeMap<String, u32>,
    /// 用户选择忽略的更新版本，更新检查不再对其提示。
    #[serde(default)]
    pub ignored_update_version: Option<String>,
    #[serde(default)]
    pub reduce_motion: bool,
    /// 标准模式相似谱面数据集的根目录。
    #[serde(default)]
    pub similarity_index_directory: Option<String>,
    /// Mania 相似谱面数据集的根目录。
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
    #[serde(default)]
    pub otd_executable_path: Option<String>,
    #[serde(default)]
    pub launch_otd_with_game: bool,
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
    /// 游戏内覆盖层（透明置顶小窗：HUD 状态 + Direct 面板 + MP 补缺）。
    #[serde(default)]
    pub overlay_enabled: bool,
    #[serde(default = "default_true")]
    pub overlay_show_with_game: bool,
    #[serde(default = "default_true")]
    pub overlay_auto_hide_on_game_exit: bool,
    /// HUD 停靠角，取值为 `TopRight` 等角名枚举字符串。
    #[serde(default = "default_overlay_corner")]
    pub overlay_corner: String,
    /// 切换覆盖层显隐的全局快捷键（默认 `Control+Shift+O`）。
    #[serde(default = "default_overlay_hotkey_toggle")]
    pub overlay_hotkey_toggle: String,
    #[serde(default = "default_true")]
    pub overlay_click_through: bool,
    #[serde(default = "default_true")]
    pub overlay_mp_auto_download: bool,
    /// 覆盖层需要数据源：游戏启动时自动拉起 tosu（托管启动不弹浏览器）。
    #[serde(default = "default_true")]
    pub overlay_start_tosu_with_game: bool,
    /// 把 HUD 装进 tosu 的游戏内渲染（独占全屏可见）。
    #[serde(default)]
    pub overlay_ingame_enabled: bool,
    /// osu! IRC 昵称（多人补缺监听；令牌存系统密钥环不落盘）。
    #[serde(default)]
    pub overlay_irc_username: Option<String>,
}

fn default_collection_backup_retention() -> usize {
    10
}

fn default_overlay_corner() -> String {
    "TopRight".into()
}

fn default_overlay_hotkey_toggle() -> String {
    "Control+Shift+O".into()
}

/// danser 渲染参数，字段逐项映射到 danser CLI 选项。
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
    /// 渲染起止时间（秒），`None` 表示整段渲染。
    #[serde(default)]
    pub start: Option<f64>,
    #[serde(default)]
    pub end: Option<f64>,
    /// 播放速度与音调倍率，`1.0` 表示不变。
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
    // `cs`/`ar`/`od`/`hp` 覆盖谱面难度参数，`None` 沿用谱面原值。
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
    /// 输出视频的分辨率（像素）、帧率与编码器名称。
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

/// 全局快捷键绑定，值为 Tauri 快捷键格式字符串（如 `Alt+1`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppKeyBindings {
    #[serde(default = "default_open_local_maps_key")]
    pub open_local_maps: String,
    #[serde(default = "default_open_trainer_key")]
    pub open_trainer: String,
    #[serde(default = "default_open_settings_key")]
    pub open_settings: String,
}

/// 相似度维度权重的来源：`Manual` 使用 `SimilarityManualWeights`，
/// `Dynamic` 由相似度引擎按查询谱面自动分配（需数据集支持）。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SimilarityWeightingPreference {
    #[default]
    Dynamic,
    Manual,
}

/// 五个标准维度（Aim/Speed/Reading/Slider/Overlap）与谱面参数维度的手动权重，
/// 数值为相对大小。
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

/// 相似谱面查询偏好：高级模式开关、权重来源与结果分页。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SimilarityPreferences {
    #[serde(default)]
    pub advanced_enabled: bool,
    #[serde(default)]
    pub mode: SimilarityWeightingPreference,
    /// 动态权重（`WeightingMode::Dynamic`）使用的谱面分段数量下界。
    #[serde(default = "default_similarity_section_range")]
    pub lower_sections: u32,
    /// 动态权重（`WeightingMode::Dynamic`）使用的谱面分段数量上界。
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

/// 内置谱面下载镜像源。
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
            show_local_beatmap_presence: true,
            local_beatmap_presence_scope: LocalPresenceScope::All,
            collection_manager_path: None,
            collection_backup_retention: default_collection_backup_retention(),
            collection_backup_directory: None,
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
            otd_executable_path: None,
            launch_otd_with_game: false,
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
            overlay_enabled: false,
            overlay_show_with_game: true,
            overlay_auto_hide_on_game_exit: true,
            overlay_corner: default_overlay_corner(),
            overlay_hotkey_toggle: default_overlay_hotkey_toggle(),
            overlay_click_through: true,
            overlay_mp_auto_download: true,
            overlay_start_tosu_with_game: true,
            overlay_ingame_enabled: false,
            overlay_irc_username: None,
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

/// OAuth 登录状态快照：凭据是否配置、是否已连接及当前用户信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthStatus {
    pub credentials_configured: bool,
    pub connected: bool,
    pub client_id: Option<String>,
    pub callback_url: String,
    pub user_id: Option<u64>,
    pub username: Option<String>,
}

/// 一次进行中的 OAuth 授权：前端打开 `authorization_url` 完成授权，
/// 超过 `expires_at` 后该链接不再有效。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingOAuth {
    pub authorization_url: String,
    pub expires_at: DateTime<Utc>,
}

/// OAuth 回调处理结果：`ok` 表示成败，失败时携带 `code` 与 `message`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthResult {
    pub ok: bool,
    pub code: String,
    pub message: String,
}

/// 用户保存的 OAuth 应用凭据：仅客户端 ID 与回调地址，不含密钥。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedCredentials {
    pub client_id: String,
    pub callback_url: String,
}

/// 断开连接结果：`revoked` 表示服务端令牌是否吊销成功，
/// `warning` 携带非致命的提示信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisconnectResult {
    pub revoked: bool,
    pub warning: Option<String>,
}

/// 持久化缓存的单条记录：原始 JSON 值与抓取时间。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheRecord {
    pub value: Value,
    pub fetched_at: DateTime<Utc>,
}

/// 落盘到应用数据目录的持久化状态：登录身份、设置、API 缓存与手动刷新时间。
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
    /// 各缓存键最近一次手动刷新的时间。
    #[serde(default)]
    pub last_manual_refresh: BTreeMap<String, DateTime<Utc>>,
}

/// 生效的令牌组：访问令牌、可选刷新令牌与过期时刻。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: DateTime<Utc>,
}

/// 令牌端点的原始响应，`expires_in` 为有效期秒数。
#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    pub expires_in: i64,
}
