//! 本地分析的领域模型：谱面 / 皮肤摘要与详情、查询条件、扫描进度、
//! 索引状态等与前端共享的序列化契约。
use std::{collections::BTreeMap, fmt};

use serde::{Deserialize, Serialize};

use crate::domain::Ruleset;

/// 本地 osu! 客户端类型，决定数据来源与目录布局。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "lowercase")]
pub enum LocalClient {
    // 官方旧版客户端：谱面 / 皮肤直接放在安装目录的 Songs / Skins 下。
    Stable,
    // 新版客户端：元数据在 client.realm，文件在 files/ 内容寻址存储。
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

/// 数据源路径的来源：`Auto` 为平台自动探测，`Override` 为用户手动指定。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceMode {
    Auto,
    Override,
}

/// 扫描结果完整度；`Partial` 表示部分资源无法枚举，不应把缺失当作不存在。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Completeness {
    Complete,
    Partial,
}

/// 单个客户端索引缓存的存储状态，反映缓存读取与迁移的健康状况。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalLibraryStorageStatus {
    pub client: LocalClient,
    /// 存储位置标识：`database`（SQLite 快照）、`json`（兼容回退）或 `unscanned`。
    pub storage: String,
    /// 索引修订号，取扫描完成时间（RFC 3339）。
    pub revision: Option<String>,
    /// 索引条目总数（含皮肤条目）。
    pub entry_count: usize,
    /// 其中谱面条目数量。
    pub beatmap_count: usize,
    /// 存储读取失败的原因。
    pub error: Option<String>,
}

/// 谱面在本地客户端中的存在性：`Unknown` 表示索引缺失或不完整，无法断言。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LocalPresenceState {
    Present,
    Missing,
    Unknown,
}

/// 单个谱面 ID 的存在性查询结果；`clients` 列出包含它的客户端。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalBeatmapPresence {
    pub beatmap_id: i32,
    pub status: LocalPresenceState,
    pub clients: Vec<LocalClient>,
}

/// 某项能力可被本地扫描器读取的可靠程度。
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
    /// 谱面列表与元数据的可读级别。
    pub beatmaps: CapabilityLevel,
    /// 难度属性（星数、PP 等）的可读级别。
    pub difficulty: CapabilityLevel,
    /// 皮肤列表的可读级别。
    pub skins: CapabilityLevel,
    /// 皮肤资源文件（图片 / 音效）的可读级别。
    pub skin_resources: CapabilityLevel,
    /// 是否可用 Realm 数据库作为权威索引（仅 lazer 为 `true`）。
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
    /// 用户手动指定的目录；`None` 表示当前走自动探测。
    pub configured_path: Option<String>,
    /// 客户端安装目录；lazer 的安装目录与数据目录不同。
    pub install_root: Option<String>,
    /// 数据根目录：stable 即安装目录，lazer 为包含 `client.realm` 的目录。
    pub data_root: Option<String>,
    /// 客户端版本（stable 取 `LastVersion`，lazer 取 `sq.version`）。
    pub version: Option<String>,
    /// 探测是否成功；失败原因见 `validation_errors`。
    pub valid: bool,
    pub validation_errors: Vec<String>,
    pub capabilities: LocalCapabilities,
    /// 最近一次成功扫描的时间；无匹配索引时为 `None`。
    pub last_scanned_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// 扫描索引中资源的稳定引用；`resource_id` 用于后续按需读取内容。
pub struct LocalResourceRef {
    /// 稳定资源标识：含客户端前缀与内容哈希，stable 另附路径短哈希。
    pub resource_id: String,
    pub client: LocalClient,
    /// 文件内容的 SHA-256（十六进制）。
    pub content_hash: String,
    /// 库内逻辑路径（`/` 分隔），用于展示与谱面集分组。
    pub logical_path: Option<String>,
}

/// 扫描期间的非致命诊断条目，不阻断索引构建。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScanDiagnostic {
    /// 稳定错误码，便于按类别归类。
    pub code: String,
    pub message: String,
    pub logical_path: Option<String>,
    pub resource_id: Option<String>,
}

/// 难度计算引擎与上游算法版本信息，随分析结果持久化以保证可追溯。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalCalculationVersion {
    pub engine: String,
    pub engine_version: String,
    pub engine_released_at: String,
    pub upstream_repository: String,
    pub upstream_revision: String,
    pub upstream_date: String,
    /// 各规则集的难度算法版本（键为 `osu`/`taiko`/`fruits`/`mania`）。
    pub ruleset_versions: BTreeMap<String, u32>,
    /// 计算假定的 mod 集合（当前为 `NoMod`）。
    pub modifiers: String,
    /// PP 计算假定（满分 / 最大连击 / 0 miss）。
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
    /// 谱面集数量是否由元数据推断（而非目录或在线 ID 分组）。
    pub beatmap_set_count_inferred: bool,
    pub skin_count: usize,
    /// 扫描枚举到的源文件总数。
    pub source_file_count: usize,
    /// 源文件总字节数。
    pub source_bytes: u64,
    pub diagnostic_count: usize,
    /// 各游戏模式的谱面数量，键为规则集名。
    pub mode_counts: BTreeMap<String, usize>,
    pub calculation: LocalCalculationVersion,
}

/// 谱面物件数量统计；`holds` 为 mania 长按物件。
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
    /// 谱面集分组键，前缀 `online:`/`folder:`/`inferred:` 区分分组来源。
    pub set_key: String,
    /// 分组是否由元数据哈希推断（无在线 ID 且非 stable 目录时发生）。
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
    /// `.osu` 格式版本号（如 v14）。
    pub format_version: i32,
    /// NoMod 星数；难度计算失败时为 `None`。
    pub stars: Option<f64>,
    /// NoMod 满分（最大连击、0 miss）PP。
    pub max_pp: Option<f64>,
    pub max_combo: Option<u32>,
    pub bpm: f64,
    /// 时长（毫秒），从首个物件到最后一物件的结束时间。
    pub length_ms: f64,
    pub object_count: usize,
    pub cs: f32,
    pub ar: f32,
    pub od: f32,
    pub hp: f32,
    /// 平均 NPS，按扣除休息时段后的活动时长计算。
    pub average_nps: f64,
    /// 峰值 NPS，按滑动的一秒窗口计算。
    pub peak_nps: f64,
    pub modified_at: Option<String>,
    /// `ready`、`skipped_suspicious` 或 `difficulty_failed`。
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
    /// `.osu` 的 `SliderMultiplier`（滑条基础速度倍率）。
    pub slider_multiplier: f64,
    /// `.osu` 的 `SliderTickRate`（每拍滑条 tick 数）。
    pub slider_tick_rate: f64,
    pub hit_objects: HitObjectCounts,
    pub break_count: usize,
    /// 休息时段总时长（毫秒）。
    pub break_duration_ms: f64,
    /// 计时点（TimingPoint）数量。
    pub timing_point_count: usize,
    /// 扣除休息时段后的活动时长（毫秒）。
    pub active_length_ms: f64,
    pub average_nps: f64,
    pub peak_nps: f64,
    /// 计算所用算法标识（见 `DIFFICULTY_ALGORITHM`）。
    pub difficulty_algorithm: String,
    pub calculation: LocalCalculationVersion,
    /// 计算发生时间（RFC 3339）。
    pub calculated_at: String,
    /// 难度曲线仅在请求详情时现场计算，列表查询不携带。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strains: Option<StrainAnalysis>,
}

/// 谱面集（一组难度）的汇总视图。
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
    /// 用于加载封面图的谱面资源 ID（如有）。
    pub background_resource_id: Option<String>,
    /// 集内各难度，会应用当前查询的过滤条件。
    pub difficulties: Vec<LocalBeatmapSummary>,
}

/// 单条难度曲线；`values` 与 `StrainAnalysis` 的固定 section 时间轴对应。
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

/// `skin.ini` 中的单个键值条目。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkinConfigEntry {
    pub key: String,
    pub value: String,
    /// 值形如 `R,G,B[,A]` 时解析出的颜色分量；非颜色值为 `None`。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<Vec<u8>>,
}

/// `skin.ini` 的一个 `[分节]` 及其键值条目。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkinConfigSection {
    pub name: String,
    pub entries: Vec<SkinConfigEntry>,
}

/// 皮肤目录的资源统计。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkinInventory {
    pub file_count: usize,
    pub total_bytes: u64,
    /// 按扩展名（小写；无扩展名记为 `"(none)"`）的文件计数。
    pub by_extension: BTreeMap<String, usize>,
}

/// 皮肤可预览资源的类型：图片或音频。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkinAssetKind {
    Image,
    Audio,
}

/// 皮肤内单个可预览资源的摘要。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalSkinAssetSummary {
    /// 形如 `skin-asset:<sha256>`，由皮肤资源 ID 与小写逻辑路径派生。
    pub resource_id: String,
    pub kind: SkinAssetKind,
    pub name: String,
    /// 相对皮肤根目录的逻辑路径（`/` 分隔）。
    pub logical_path: String,
    pub extension: String,
    pub size: u64,
    /// 按路径关键词推断的中文分类（如 “光标” “Mania” “击打音效”）。
    pub category: String,
}

/// 皮肤预览索引：按类型拆分的可预览资源列表。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalSkinPreview {
    pub skin_resource_id: String,
    pub completeness: Completeness,
    pub images: Vec<LocalSkinAssetSummary>,
    pub sounds: Vec<LocalSkinAssetSummary>,
}

/// 皮肤资源的内联载荷，`data_url` 可直接用于前端展示。
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
    /// 配置中是否含 `[Mania]` 分节。
    pub has_mania_config: bool,
    /// 资源文件数；目录不可访问（`Partial`）时为 `None`。
    pub resource_count: Option<usize>,
    /// 资源总字节数；同样可能为 `None`。
    pub total_bytes: Option<u64>,
    pub modified_at: Option<String>,
    /// 按原始顺序取前 8 个颜色条目的分量。
    pub accent_colors: Vec<Vec<u8>>,
}

/// 皮肤详情：配置分节、可选资源清单与给用户的提示。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalSkinDetail {
    pub summary: LocalSkinSummary,
    pub sections: Vec<SkinConfigSection>,
    /// 资源统计；皮肤根目录不可访问时为 `None`。
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

/// 谱面列表的排序字段。
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
    /// 全部可选排序字段，供前端选项与校验使用。
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
    /// 过滤是否已提交官网（以存在在线谱面集 ID 判断）；`None` 不过滤。
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

/// 皮肤列表的排序字段。
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
    /// 全部可选排序字段，供前端选项与校验使用。
    pub const ALL: [Self; 4] = [Self::Name, Self::Author, Self::Size, Self::ModifiedAt];
}

/// 皮肤列表的筛选、排序与分页条件。
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
    /// 阶段名（`discovery`、`indexing`、`beatmaps`、`difficulty`、`skins`、`finalizing`）。
    pub phase: String,
    pub processed: usize,
    pub total: usize,
    pub percent: f64,
}

/// 索引后台加载的整体阶段。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalIndexLoadPhase {
    Loading,
    Ready,
    Error,
}

/// 索引加载状态与各客户端的监听 / 增量信息。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalIndexLoadStatus {
    pub phase: LocalIndexLoadPhase,
    pub error: Option<String>,
    /// 每个客户端的后台监听与增量索引状态。
    #[serde(default)]
    pub clients: BTreeMap<LocalClient, LocalIndexClientStatus>,
}

/// 谱面音频试听载荷（base64 内联传输）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalBeatmapAudioPayload {
    pub mime_type: String,
    pub bytes_base64: String,
    /// 试听起点（毫秒），来自 `.osu` 的 `PreviewTime`。
    pub preview_time_ms: f64,
}

/// 备份恢复用的单个谱面集本地快照：展示元数据加全部本地难度。
#[derive(Debug, Clone)]
pub struct LocalBeatmapSetSnapshot {
    /// 在线谱面集 ID。
    pub beatmapset_id: i32,
    /// 展示标题（优先 Unicode，缺失回退 ASCII）。
    pub title: String,
    /// 展示艺术家（优先 Unicode，缺失回退 ASCII）。
    pub artist: String,
    pub creator: String,
    /// 命中的本地客户端；同一集合 Stable 优先于 Lazer。
    pub client: LocalClient,
    /// 该集合在本地索引中的全部难度。
    pub difficulties: Vec<LocalBeatmapDifficultyRef>,
}

/// 快照内的单个难度引用：收藏条目构建所需的字段。
#[derive(Debug, Clone)]
pub struct LocalBeatmapDifficultyRef {
    pub beatmap_id: Option<i32>,
    /// `.osu` 文件内容 MD5（小写）；与 collection.db 校验和一致。
    pub checksum: Option<String>,
    /// 小写模式名（`osu`/`taiko`/`fruits`/`mania`）。
    pub ruleset: Option<String>,
    pub difficulty_name: String,
}

/// 背景图输出尺寸档位。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundSize {
    /// 缩略图：缩放至 960×540 以内。
    #[default]
    Thumbnail,
    /// 舞台尺寸：缩放至 1920×1080 以内。
    Stage,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct LocalIndexClientStatus {
    /// `idle`、`watching`、`pending`、`scanning` 或 `error`。
    pub phase: String,
    /// 监听到但尚未触发增量扫描的变更次数。
    pub pending_changes: usize,
    pub last_change_at: Option<String>,
    pub last_scan_at: Option<String>,
    /// 增量扫描统计：新增 / 修改 / 删除 / 沿用未变的条目数。
    pub added: usize,
    pub modified: usize,
    pub removed: usize,
    pub reused: usize,
}

impl Default for LocalIndexClientStatus {
    fn default() -> Self {
        Self {
            phase: "idle".into(),
            pending_changes: 0,
            last_change_at: None,
            last_scan_at: None,
            added: 0,
            modified: 0,
            removed: 0,
            reused: 0,
        }
    }
}
/// 背景图样本引用；前端凭 `(client, resource_id)` 再请求实际图片。
#[derive(Debug, Clone, serde::Serialize)]
pub struct LocalArtwork {
    pub client: LocalClient,
    pub resource_id: String,
}
