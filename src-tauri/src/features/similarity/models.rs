//! 相似谱面功能的前后端数据契约：索引状态、查询/推荐请求与响应结构。
//! 这些类型直接参与 Tauri 命令的序列化，字段名与标签值属于对外契约。

use std::collections::BTreeMap;

use osu_difficulty_runtime::{
    BaseFeatures, DifficultyVector, DynamicWeightProfile, ManiaBaseFeatures, ManiaDifficultyVector,
    ManiaDistanceComponents, ManiaGameMod, ManiaModeFamily, ManiaPattern, ManiaStyleVector,
    QueryFilters, WeightingMode,
};
use serde::{Deserialize, Serialize};

use crate::domain::Ruleset;

/// 索引可用性状态：未配置、目录缺失、文件无效、版本不兼容、就绪、模式不支持。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SimilarityIndexState {
    Unconfigured,
    Missing,
    Invalid,
    Incompatible,
    Ready,
    Unsupported,
}

/// 索引体检结果：状态加数据集元信息（规模、版本、算法标识），供设置页展示。
#[derive(Debug, Clone, Serialize)]
pub struct SimilarityIndexStatus {
    pub ruleset: Ruleset,
    pub state: SimilarityIndexState,
    pub directory: Option<String>,
    pub message: String,
    /// 索引内的特征记录总数。
    pub record_count: Option<usize>,
    /// Mania 按键数（4/6/7）统计的记录数；standard 恒为 null。
    pub records_by_key_count: Option<BTreeMap<u8, usize>>,
    /// 生成数据集所用分析器的版本号。
    pub analyzer_version: Option<u32>,
    /// 特征归一化参数的版本号。
    pub normalization_version: Option<u32>,
    /// 数据集算法标识，如 Mania 的 `mania-roxy-interlude-similarity-v1`。
    pub algorithm_id: Option<String>,
    /// 数据集收录数据的截止时间戳。
    pub data_cutoff_at: Option<i64>,
    /// 当前实现恒为 false：standard 动态权重已停用。
    pub supports_dynamic_weighting: bool,
}

impl SimilarityIndexStatus {
    /// 构造“尚未配置目录”的正常状态。
    pub fn unconfigured(ruleset: Ruleset) -> Self {
        Self {
            ruleset,
            state: SimilarityIndexState::Unconfigured,
            directory: None,
            message: "尚未配置本地相似谱面索引。".into(),
            record_count: None,
            records_by_key_count: None,
            analyzer_version: None,
            normalization_version: None,
            algorithm_id: None,
            data_cutoff_at: None,
            supports_dynamic_weighting: false,
        }
    }

    /// 构造 Taiko / Catch 等未实现模式的显式不支持状态。
    pub fn unsupported(ruleset: Ruleset) -> Self {
        Self {
            ruleset,
            state: SimilarityIndexState::Unsupported,
            directory: None,
            message: match ruleset {
                Ruleset::Taiko => "相似谱面暂不支持 osu!taiko。",
                Ruleset::Fruits => "相似谱面暂不支持 osu!catch。",
                _ => "当前模式暂不支持相似谱面。",
            }
            .into(),
            record_count: None,
            records_by_key_count: None,
            analyzer_version: None,
            normalization_version: None,
            algorithm_id: None,
            data_cutoff_at: None,
            supports_dynamic_weighting: false,
        }
    }
}

/// 查询目标的来源：Beatmap ID（数字或 osu.ppy.sh 链接）或本地 `.osu` 文件路径。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SimilaritySource {
    BeatmapId { value: String },
    LocalFile { path: String },
}

/// 相似查询请求：按 `ruleset` 标签区分 standard 与 Mania 两套互不适用的参数。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "ruleset", rename_all = "lowercase", deny_unknown_fields)]
pub enum SimilarityQueryRequest {
    Osu {
        source: SimilaritySource,
        /// 手动权重（五维难度权重 + 参数权重）；动态权重会被校验层拒绝。
        weighting: WeightingMode,
        /// 元数据范围筛选（星数、AR、CS、OD、BPM、时长、物件密度与占比）。
        #[serde(default)]
        filters: QueryFilters,
        /// 期望返回的结果条数，合法范围 5..=50。
        result_limit: usize,
    },
    Mania {
        source: SimilaritySource,
        /// 期望返回的结果条数，合法范围 5..=50。
        result_limit: usize,
        /// 目标使用的 Mod 变体，决定读取 NM/DT/HT 中哪套预计算特征。
        #[serde(default)]
        target_mod: ManiaGameMod,
        /// 候选池允许出现的 Mod 集合，默认仅 NM。
        #[serde(default = "default_mania_candidate_mods")]
        candidate_mods: Vec<ManiaGameMod>,
    },
}

/// 候选 Mod 池缺省值：仅 NM。
fn default_mania_candidate_mods() -> Vec<ManiaGameMod> {
    vec![ManiaGameMod::Nm]
}

impl SimilarityQueryRequest {
    pub const fn ruleset(&self) -> Ruleset {
        match self {
            Self::Osu { .. } => Ruleset::Osu,
            Self::Mania { .. } => Ruleset::Mania,
        }
    }

    pub const fn source(&self) -> &SimilaritySource {
        match self {
            Self::Osu { source, .. } | Self::Mania { source, .. } => source,
        }
    }
}

/// standard 谱面卡片：展示用元数据加特征向量。
#[derive(Debug, Clone, Serialize)]
pub struct SimilarityBeatmap {
    pub ruleset: Ruleset,
    pub beatmap_id: u64,
    pub beatmapset_id: u64,
    pub artist: String,
    pub title: String,
    pub version: String,
    pub creator: String,
    pub online_url: String,
    pub star_rating: Option<f32>,
    /// 五维难度向量（Aim / Speed / Reading / Slider / Overlap）。
    pub difficulty: DifficultyVector,
    /// 基础参数特征，参与 `base_distance` 的计算。
    pub base: BaseFeatures,
}

/// 查询目标：索引内或即时分析出的谱面，附来源与数据集版本信息。
#[derive(Debug, Clone, Serialize)]
pub struct SimilarityTarget {
    #[serde(flatten)]
    pub beatmap: SimilarityBeatmap,
    /// 来源标签：`index` / `online` / `local_file`。
    pub source: String,
    pub analyzer_version: u32,
    pub normalization_version: u32,
}

/// 单条相似候选，按 `final_distance` 升序排列。
#[derive(Debug, Clone, Serialize)]
pub struct SimilarityResult {
    #[serde(flatten)]
    pub beatmap: SimilarityBeatmap,
    /// 加权后的总距离，越小越相似。
    pub final_distance: f32,
    /// 五维难度向量上的距离分量。
    pub difficulty_distance: f32,
    /// 基础参数特征上的距离分量。
    pub base_distance: f32,
}

/// Mania 谱面卡片：键数、模式家族、主导键型与难度/风格特征向量。
#[derive(Debug, Clone, Serialize)]
pub struct ManiaSimilarityBeatmap {
    pub ruleset: Ruleset,
    pub beatmap_id: u64,
    pub beatmapset_id: u64,
    pub artist: String,
    pub title: String,
    pub version: String,
    pub creator: String,
    pub online_url: String,
    /// 键数，数据集仅收录 4 / 6 / 7。
    pub key_count: u8,
    /// 模式家族（序列化为 snake_case，如 `rc`）。
    pub family: ManiaModeFamily,
    /// 主导键型（如 `stream`、`chordstream`）。
    pub pattern: ManiaPattern,
    /// 难度特征向量。
    pub difficulty: ManiaDifficultyVector,
    /// 风格特征向量。
    pub style: ManiaStyleVector,
    /// 基础特征。
    pub base: ManiaBaseFeatures,
    pub difficulty_percentile: f32,
    pub difficulty_band: u8,
    /// 该记录对应的 Mod 变体（NM / DT / HT）。
    pub game_mod: ManiaGameMod,
    /// 键型视图；旧数据集或未生成键型记录时为 null。
    #[serde(default)]
    pub pattern_view: Option<ManiaPatternView>,
}

/// `mania_map_analyser` 键型分析结果的展示视图。
#[derive(Debug, Clone, Serialize)]
pub struct ManiaPatternView {
    /// mania_map_analyser 的分类结果，例如 `Shield`、`Jumpstream/Handstream Tech`。
    pub category: String,
    /// `RC` / `HB` / `Mix` / `LN`。
    pub mode_tag: String,
    /// 六类覆盖率，顺序为 Stream、Chordstream、Jacks、Coordination、Density、Wildcard。
    pub coverage: [f32; 6],
    /// 键型构成条目，用于条形图展示。
    pub bars: Vec<ManiaPatternBarView>,
    /// 细分子类型及其占比。
    pub subtypes: Vec<(String, f32)>,
    /// LN 音符占总音符的比例。
    #[serde(rename = "ln_note_ratio")]
    pub ln_note_ratio: f32,
    /// 平均 NPS、峰值 NPS、持续 NPS、最长持续段秒数。
    pub intensity: [f32; 4],
    /// 持续段占比、最长持续段占全谱比例、空窗比例。
    pub temporal: [f32; 3],
    /// 谱面时长（秒）。
    pub duration_seconds: f32,
    /// 变速（SV）总量。
    pub sv_amount: f32,
}

/// 单根键型构成条目。
#[derive(Debug, Clone, Serialize)]
pub struct ManiaPatternBarView {
    /// 键型名称。
    pub pattern: String,
    /// 该键型的原始量。
    pub amount: f32,
    /// 相对最长条的比例，仅用于显示；覆盖率本身不做归一化。
    pub relative: f32,
    /// 细分类型及其占比。
    pub specific_types: Vec<(String, f32)>,
}

/// Mania 查询目标：种子谱面加来源与数据集版本信息。
#[derive(Debug, Clone, Serialize)]
pub struct ManiaSimilarityTarget {
    #[serde(flatten)]
    pub beatmap: ManiaSimilarityBeatmap,
    pub source: String,
    pub analyzer_version: u32,
    pub normalization_version: u32,
}

/// 单条 Mania 相似候选，按 `final_distance` 升序排列。
#[derive(Debug, Clone, Serialize)]
pub struct ManiaSimilarityResult {
    #[serde(flatten)]
    pub beatmap: ManiaSimilarityBeatmap,
    /// 总距离，越小越相似。
    pub final_distance: f32,
    /// 分维度距离分量（如 `skill`）。
    pub distance_components: ManiaDistanceComponents,
}

/// 查询响应：目标谱面信息加相似候选列表。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "ruleset", rename_all = "lowercase")]
pub enum SimilarityQueryResponse {
    Osu {
        target: SimilarityTarget,
        results: Vec<SimilarityResult>,
        /// 查询附带的全局权重画像。
        dynamic_profile: Option<DynamicWeightProfile>,
    },
    Mania {
        target: ManiaSimilarityTarget,
        results: Vec<ManiaSimilarityResult>,
    },
}

/// 推荐参考成绩的类型：最近通过成绩或 BP（最好成绩）。
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SimilarityRecommendationKind {
    Recent,
    Best,
}

/// 推荐请求：同样按 `ruleset` 区分两套参数，种子来自玩家成绩。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "ruleset", rename_all = "lowercase", deny_unknown_fields)]
pub enum SimilarityRecommendationRequest {
    Osu {
        kind: SimilarityRecommendationKind,
        weighting: WeightingMode,
        #[serde(default)]
        filters: QueryFilters,
        result_limit: usize,
        /// 参考成绩数量上限，缺省 50（clamp 到 1..=50）。
        #[serde(default)]
        seed_limit: Option<usize>,
        /// 需要排除的谱面 ID（如已玩过的谱面）。
        #[serde(default)]
        excluded_beatmap_ids: Vec<u64>,
    },
    Mania {
        kind: SimilarityRecommendationKind,
        result_limit: usize,
        /// 参考成绩数量上限，缺省 50（clamp 到 1..=50）。
        #[serde(default)]
        seed_limit: Option<usize>,
        /// 需要排除的谱面 ID（如已玩过的谱面）。
        #[serde(default)]
        excluded_beatmap_ids: Vec<u64>,
        /// 候选池允许出现的 Mod 集合，默认仅 NM。
        #[serde(default = "default_mania_candidate_mods")]
        candidate_mods: Vec<ManiaGameMod>,
    },
}

impl SimilarityRecommendationRequest {
    pub const fn ruleset(&self) -> Ruleset {
        match self {
            Self::Osu { .. } => Ruleset::Osu,
            Self::Mania { .. } => Ruleset::Mania,
        }
    }

    pub const fn kind(&self) -> SimilarityRecommendationKind {
        match self {
            Self::Osu { kind, .. } | Self::Mania { kind, .. } => *kind,
        }
    }

    pub const fn result_limit(&self) -> usize {
        match self {
            Self::Osu { result_limit, .. } | Self::Mania { result_limit, .. } => *result_limit,
        }
    }

    pub const fn seed_limit(&self) -> Option<usize> {
        match self {
            Self::Osu { seed_limit, .. } | Self::Mania { seed_limit, .. } => *seed_limit,
        }
    }

    pub fn excluded_beatmap_ids(&self) -> &[u64] {
        match self {
            Self::Osu {
                excluded_beatmap_ids,
                ..
            }
            | Self::Mania {
                excluded_beatmap_ids,
                ..
            } => excluded_beatmap_ids,
        }
    }
}

/// standard 推荐条目：候选结果加触发推荐的种子谱面。
#[derive(Debug, Clone, Serialize)]
pub struct SimilarityRecommendationResult {
    #[serde(flatten)]
    pub result: SimilarityResult,
    /// 该候选是哪张种子谱面的最近邻。
    pub recommended_by: SimilarityBeatmap,
}

/// Mania 推荐条目：候选结果加触发推荐的种子谱面。
#[derive(Debug, Clone, Serialize)]
pub struct ManiaSimilarityRecommendationResult {
    #[serde(flatten)]
    pub result: ManiaSimilarityResult,
    /// 该候选是哪张种子谱面的最近邻。
    pub recommended_by: ManiaSimilarityBeatmap,
}

/// Mania 推荐按键数独立分组的一组结果。
#[derive(Debug, Clone, Serialize)]
pub struct ManiaSimilarityRecommendationGroup {
    /// 分组键数（4 / 6 / 7）。
    pub key_count: u8,
    /// 该键数下实际参与的种子数。
    pub seed_count: usize,
    pub results: Vec<ManiaSimilarityRecommendationResult>,
}

/// 推荐响应：聚合所有种子并去重后的候选列表与统计信息。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "ruleset", rename_all = "lowercase")]
pub enum SimilarityRecommendationResponse {
    Osu {
        kind: SimilarityRecommendationKind,
        /// 实际参与查询的种子数。
        seed_count: usize,
        /// 因构建目标失败（获取或分析失败）而跳过的种子数。
        skipped_seed_count: usize,
        results: Vec<SimilarityRecommendationResult>,
        /// 各种子的动态权重画像列表。
        dynamic_profiles: Vec<SimilaritySeedDynamicProfile>,
    },
    Mania {
        kind: SimilarityRecommendationKind,
        /// 实际参与查询的种子数（等于各分组 seed_count 之和）。
        seed_count: usize,
        /// 跳过的种子数（含 Mod 不可用而无法作参考的成绩）。
        skipped_seed_count: usize,
        groups: Vec<ManiaSimilarityRecommendationGroup>,
    },
}

/// 单个种子的动态权重画像：种子谱面 ID 加扁平化的画像字段。
#[derive(Debug, Clone, Serialize)]
pub struct SimilaritySeedDynamicProfile {
    pub seed_beatmap_id: u64,
    #[serde(flatten)]
    pub profile: DynamicWeightProfile,
}

#[cfg(test)]
mod tests {
    use super::*;

    // 状态序列化必须包含 ruleset 与按键数分布（前端契约）。
    #[test]
    fn status_exposes_ruleset_and_key_distribution_contract() {
        let status = SimilarityIndexStatus {
            ruleset: Ruleset::Mania,
            state: SimilarityIndexState::Ready,
            directory: Some("M:/mania".into()),
            message: "ready".into(),
            record_count: Some(23_551),
            records_by_key_count: Some(BTreeMap::from([(4, 18_550), (6, 800), (7, 4_201)])),
            analyzer_version: Some(1),
            normalization_version: Some(1),
            algorithm_id: Some("mania-roxy-interlude-similarity-v1".into()),
            data_cutoff_at: None,
            supports_dynamic_weighting: false,
        };
        let value = serde_json::to_value(status).expect("serialize status");
        assert_eq!(value["ruleset"], "mania");
        assert_eq!(value["records_by_key_count"]["4"], 18_550);
        assert_eq!(value["supports_dynamic_weighting"], false);
    }

    // 推荐响应按 ruleset 判别，Mania 分支不带 dynamic_profiles 字段。
    #[test]
    fn recommendation_responses_are_discriminated_by_ruleset() {
        let response = SimilarityRecommendationResponse::Mania {
            kind: SimilarityRecommendationKind::Recent,
            seed_count: 0,
            skipped_seed_count: 2,
            groups: vec![],
        };
        let value = serde_json::to_value(response).expect("serialize response");
        assert_eq!(value["ruleset"], "mania");
        assert!(value.get("dynamic_profiles").is_none());
        assert_eq!(value["skipped_seed_count"], 2);
    }

    // Taiko / Catch 的状态必须是显式 unsupported。
    #[test]
    fn unsupported_status_is_explicit_for_unimplemented_rulesets() {
        for ruleset in [Ruleset::Taiko, Ruleset::Fruits] {
            let status = SimilarityIndexStatus::unsupported(ruleset);
            assert_eq!(status.state, SimilarityIndexState::Unsupported);
            assert_eq!(status.ruleset, ruleset);
            assert!(status.message.contains("暂不支持"));
        }
    }

    // 家族与键型枚举的 snake_case 序列化值是对外契约。
    #[test]
    fn mania_family_and_pattern_use_stable_snake_case_wire_values() {
        assert_eq!(
            serde_json::to_value(ManiaModeFamily::Rc).expect("family"),
            "rc"
        );
        assert_eq!(
            serde_json::to_value(ManiaPattern::Chordstream).expect("pattern"),
            "chordstream"
        );
    }
}
