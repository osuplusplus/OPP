use std::{collections::BTreeMap, fmt};

use serde::{Deserialize, Serialize};

use crate::models::Ruleset;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "lowercase")]
pub enum LocalClient {
    Stable,
    Lazer,
}

impl fmt::Display for LocalClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Stable => "stable",
            Self::Lazer => "lazer",
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceMode {
    Auto,
    Override,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Completeness {
    Complete,
    Partial,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CapabilityLevel {
    Full,
    Partial,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// 不同 osu! 客户端可被本地扫描器可靠读取的功能范围。
///
/// `Partial` 表示可读取部分数据，但不应将缺失数据当作不存在。
pub struct LocalCapabilities {
    pub beatmaps: CapabilityLevel,
    pub difficulty: CapabilityLevel,
    pub skins: CapabilityLevel,
    pub skin_resources: CapabilityLevel,
    pub realm_index: bool,
}

impl LocalCapabilities {
    pub fn for_client(client: LocalClient) -> Self {
        match client {
            LocalClient::Stable => Self {
                beatmaps: CapabilityLevel::Full,
                difficulty: CapabilityLevel::Full,
                skins: CapabilityLevel::Full,
                skin_resources: CapabilityLevel::Full,
                realm_index: false,
            },
            // Lazer 以 client.realm 为权威索引：谱面 / 皮肤 / 资源均完整可用。
            LocalClient::Lazer => Self {
                beatmaps: CapabilityLevel::Full,
                difficulty: CapabilityLevel::Full,
                skins: CapabilityLevel::Full,
                skin_resources: CapabilityLevel::Full,
                realm_index: true,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// 某个客户端本地数据源的解析结果与可用状态，直接提供给前端展示。
pub struct LocalSourceStatus {
    pub client: LocalClient,
    pub mode: SourceMode,
    pub configured_path: Option<String>,
    pub install_root: Option<String>,
    pub data_root: Option<String>,
    pub version: Option<String>,
    pub valid: bool,
    pub validation_errors: Vec<String>,
    pub capabilities: LocalCapabilities,
    pub last_scanned_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// 扫描索引中资源的稳定引用；`resource_id` 用于后续按需读取内容。
pub struct LocalResourceRef {
    pub resource_id: String,
    pub client: LocalClient,
    pub content_hash: String,
    pub logical_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScanDiagnostic {
    pub code: String,
    pub message: String,
    pub logical_path: Option<String>,
    pub resource_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalCalculationVersion {
    pub engine: String,
    pub engine_version: String,
    pub engine_released_at: String,
    pub upstream_repository: String,
    pub upstream_revision: String,
    pub upstream_date: String,
    pub ruleset_versions: BTreeMap<String, u32>,
    pub modifiers: String,
    pub performance_assumption: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// 一次扫描生成的库级汇总，包含数据完整度和统计口径。
pub struct LocalLibrarySummary {
    pub client: LocalClient,
    pub completeness: Completeness,
    pub source_root: String,
    pub scanned_at: String,
    pub beatmap_count: usize,
    pub beatmap_set_count: usize,
    pub beatmap_set_count_inferred: bool,
    pub skin_count: usize,
    pub source_file_count: usize,
    pub source_bytes: u64,
    pub diagnostic_count: usize,
    pub mode_counts: BTreeMap<String, usize>,
    pub calculation: LocalCalculationVersion,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct HitObjectCounts {
    pub circles: usize,
    pub sliders: usize,
    pub spinners: usize,
    pub holds: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// 谱面的轻量列表项；详情、背景与 strain 数据均通过资源引用按需获取。
pub struct LocalBeatmapSummary {
    pub resource: LocalResourceRef,
    pub set_key: String,
    pub set_grouping_inferred: bool,
    pub beatmap_id: Option<i32>,
    pub beatmap_set_id: Option<i32>,
    pub title: String,
    pub title_unicode: String,
    pub artist: String,
    pub artist_unicode: String,
    pub creator: String,
    pub difficulty_name: String,
    pub ruleset: Ruleset,
    pub format_version: i32,
    pub stars: Option<f64>,
    pub max_pp: Option<f64>,
    pub max_combo: Option<u32>,
    pub bpm: f64,
    pub length_ms: f64,
    pub object_count: usize,
    pub cs: f32,
    pub ar: f32,
    pub od: f32,
    pub hp: f32,
    pub average_nps: f64,
    pub peak_nps: f64,
    pub modified_at: Option<String>,
    pub analysis_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// 单张谱面的完整分析结果，保留计算时使用的算法版本以便结果可追溯。
pub struct LocalBeatmapDetail {
    pub summary: LocalBeatmapSummary,
    pub source: String,
    pub tags: String,
    pub background_file: String,
    pub audio_file: String,
    pub cs: f32,
    pub ar: f32,
    pub od: f32,
    pub hp: f32,
    pub slider_multiplier: f64,
    pub slider_tick_rate: f64,
    pub hit_objects: HitObjectCounts,
    pub break_count: usize,
    pub break_duration_ms: f64,
    pub timing_point_count: usize,
    pub active_length_ms: f64,
    pub average_nps: f64,
    pub peak_nps: f64,
    pub difficulty_algorithm: String,
    pub calculation: LocalCalculationVersion,
    pub calculated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strains: Option<StrainAnalysis>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalBeatmapSetSummary {
    pub set_key: String,
    pub completeness: Completeness,
    pub grouping_inferred: bool,
    pub beatmap_set_id: Option<i32>,
    pub title: String,
    pub title_unicode: String,
    pub artist: String,
    pub artist_unicode: String,
    pub creators: Vec<String>,
    pub min_stars: Option<f64>,
    pub max_stars: Option<f64>,
    pub bpm: f64,
    pub length_ms: f64,
    pub object_count: usize,
    pub modified_at: Option<String>,
    pub background_resource_id: Option<String>,
    pub difficulties: Vec<LocalBeatmapSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StrainSeries {
    pub key: String,
    pub values: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// 按固定时间段切分的难度曲线，`series` 的键会随游戏模式变化。
pub struct StrainAnalysis {
    /// 首个物件相对音频起点的绝对时间。前端用它把 section 曲线放回编辑器时间轴。
    pub first_object_time_ms: f64,
    /// 第一个固定 section 的起点；第一个数据点位于该值加上 `section_length_ms`。
    pub section_start_time_ms: f64,
    pub section_length_ms: f64,
    pub series: Vec<StrainSeries>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkinConfigEntry {
    pub key: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkinConfigSection {
    pub name: String,
    pub entries: Vec<SkinConfigEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkinInventory {
    pub file_count: usize,
    pub total_bytes: u64,
    pub by_extension: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkinAssetKind {
    Image,
    Audio,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalSkinAssetSummary {
    pub resource_id: String,
    pub kind: SkinAssetKind,
    pub name: String,
    pub logical_path: String,
    pub extension: String,
    pub size: u64,
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalSkinPreview {
    pub skin_resource_id: String,
    pub completeness: Completeness,
    pub images: Vec<LocalSkinAssetSummary>,
    pub sounds: Vec<LocalSkinAssetSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalSkinAssetPayload {
    pub resource_id: String,
    pub kind: SkinAssetKind,
    pub mime_type: String,
    pub data_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// 皮肤的列表摘要；当客户端无法枚举完整资源时以 `completeness` 标明。
pub struct LocalSkinSummary {
    pub resource: LocalResourceRef,
    pub completeness: Completeness,
    pub name: String,
    pub author: String,
    pub version: String,
    pub section_count: usize,
    pub has_mania_config: bool,
    pub resource_count: Option<usize>,
    pub total_bytes: Option<u64>,
    pub modified_at: Option<String>,
    pub accent_colors: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalSkinDetail {
    pub summary: LocalSkinSummary,
    pub sections: Vec<SkinConfigSection>,
    pub inventory: Option<SkinInventory>,
    pub notice: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// 通用偏移量分页响应，`total` 是过滤后但分页前的总数。
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    #[default]
    Asc,
    Desc,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "snake_case")]
pub enum BeatmapSort {
    #[default]
    Title,
    Artist,
    Creator,
    Stars,
    Bpm,
    Length,
    ObjectCount,
    ModifiedAt,
}

impl BeatmapSort {
    pub const ALL: [Self; 8] = [
        Self::Title,
        Self::Artist,
        Self::Creator,
        Self::Stars,
        Self::Bpm,
        Self::Length,
        Self::ObjectCount,
        Self::ModifiedAt,
    ];
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
/// 本地谱面列表的筛选、排序与分页条件。
pub struct BeatmapQuery {
    pub client: LocalClient,
    pub search: String,
    pub rulesets: Vec<Ruleset>,
    pub min_stars: Option<f64>,
    pub max_stars: Option<f64>,
    pub min_bpm: Option<f64>,
    pub max_bpm: Option<f64>,
    pub min_length_ms: Option<f64>,
    pub max_length_ms: Option<f64>,
    pub min_objects: Option<usize>,
    pub max_objects: Option<usize>,
    pub min_ar: Option<f32>,
    pub max_ar: Option<f32>,
    pub min_cs: Option<f32>,
    pub max_cs: Option<f32>,
    pub min_od: Option<f32>,
    pub max_od: Option<f32>,
    pub submitted: Option<bool>,
    pub sort: BeatmapSort,
    pub direction: SortDirection,
    pub offset: usize,
    pub limit: usize,
}

impl Default for BeatmapQuery {
    fn default() -> Self {
        Self {
            client: LocalClient::Stable,
            search: String::new(),
            rulesets: Vec::new(),
            min_stars: None,
            max_stars: None,
            min_bpm: None,
            max_bpm: None,
            min_length_ms: None,
            max_length_ms: None,
            min_objects: None,
            max_objects: None,
            min_ar: None,
            max_ar: None,
            min_cs: None,
            max_cs: None,
            min_od: None,
            max_od: None,
            submitted: None,
            sort: BeatmapSort::Title,
            direction: SortDirection::Asc,
            offset: 0,
            limit: 100,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "snake_case")]
pub enum SkinSort {
    #[default]
    Name,
    Author,
    Size,
    ModifiedAt,
}

impl SkinSort {
    pub const ALL: [Self; 4] = [Self::Name, Self::Author, Self::Size, Self::ModifiedAt];
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SkinQuery {
    pub client: LocalClient,
    pub search: String,
    pub sort: SkinSort,
    pub direction: SortDirection,
    pub offset: usize,
    pub limit: usize,
}

impl Default for SkinQuery {
    fn default() -> Self {
        Self {
            client: LocalClient::Stable,
            search: String::new(),
            sort: SkinSort::Name,
            direction: SortDirection::Asc,
            offset: 0,
            limit: 100,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// 扫描期间发往前端的单调进度快照，百分比不会因阶段切换而倒退。
pub struct LocalScanProgress {
    pub client: LocalClient,
    pub phase: String,
    pub processed: usize,
    pub total: usize,
    pub percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalIndexLoadPhase {
    Loading,
    Ready,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalIndexLoadStatus {
    pub phase: LocalIndexLoadPhase,
    pub error: Option<String>,
}
