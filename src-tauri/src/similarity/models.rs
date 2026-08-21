use std::collections::BTreeMap;

use osu_difficulty_runtime::{
    BaseFeatures, DifficultyVector, DynamicWeightProfile, ManiaBaseFeatures, ManiaDifficultyVector,
    ManiaDistanceComponents, ManiaModeFamily, ManiaPattern, ManiaStyleVector, QueryFilters,
    WeightingMode,
};
use serde::{Deserialize, Serialize};

use crate::app::models::Ruleset;

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

#[derive(Debug, Clone, Serialize)]
pub struct SimilarityIndexStatus {
    pub ruleset: Ruleset,
    pub state: SimilarityIndexState,
    pub directory: Option<String>,
    pub message: String,
    pub record_count: Option<usize>,
    pub records_by_key_count: Option<BTreeMap<u8, usize>>,
    pub analyzer_version: Option<u32>,
    pub normalization_version: Option<u32>,
    pub algorithm_id: Option<String>,
    pub data_cutoff_at: Option<i64>,
    pub supports_dynamic_weighting: bool,
}

impl SimilarityIndexStatus {
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

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SimilaritySource {
    BeatmapId { value: String },
    LocalFile { path: String },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "ruleset", rename_all = "lowercase", deny_unknown_fields)]
pub enum SimilarityQueryRequest {
    Osu {
        source: SimilaritySource,
        weighting: WeightingMode,
        #[serde(default)]
        filters: QueryFilters,
        result_limit: usize,
    },
    Mania {
        source: SimilaritySource,
        result_limit: usize,
    },
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
    pub difficulty: DifficultyVector,
    pub base: BaseFeatures,
}

#[derive(Debug, Clone, Serialize)]
pub struct SimilarityTarget {
    #[serde(flatten)]
    pub beatmap: SimilarityBeatmap,
    pub source: String,
    pub analyzer_version: u32,
    pub normalization_version: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct SimilarityResult {
    #[serde(flatten)]
    pub beatmap: SimilarityBeatmap,
    pub final_distance: f32,
    pub difficulty_distance: f32,
    pub base_distance: f32,
}

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
    pub key_count: u8,
    pub family: ManiaModeFamily,
    pub pattern: ManiaPattern,
    pub difficulty: ManiaDifficultyVector,
    pub style: ManiaStyleVector,
    pub base: ManiaBaseFeatures,
    pub difficulty_percentile: f32,
    pub difficulty_band: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManiaSimilarityTarget {
    #[serde(flatten)]
    pub beatmap: ManiaSimilarityBeatmap,
    pub source: String,
    pub analyzer_version: u32,
    pub normalization_version: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManiaSimilarityResult {
    #[serde(flatten)]
    pub beatmap: ManiaSimilarityBeatmap,
    pub final_distance: f32,
    pub distance_components: ManiaDistanceComponents,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "ruleset", rename_all = "lowercase")]
pub enum SimilarityQueryResponse {
    Osu {
        target: SimilarityTarget,
        results: Vec<SimilarityResult>,
        dynamic_profile: Option<DynamicWeightProfile>,
    },
    Mania {
        target: ManiaSimilarityTarget,
        results: Vec<ManiaSimilarityResult>,
    },
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SimilarityRecommendationKind {
    Recent,
    Best,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "ruleset", rename_all = "lowercase", deny_unknown_fields)]
pub enum SimilarityRecommendationRequest {
    Osu {
        kind: SimilarityRecommendationKind,
        weighting: WeightingMode,
        #[serde(default)]
        filters: QueryFilters,
        result_limit: usize,
        #[serde(default)]
        seed_limit: Option<usize>,
        #[serde(default)]
        excluded_beatmap_ids: Vec<u64>,
    },
    Mania {
        kind: SimilarityRecommendationKind,
        result_limit: usize,
        #[serde(default)]
        seed_limit: Option<usize>,
        #[serde(default)]
        excluded_beatmap_ids: Vec<u64>,
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

#[derive(Debug, Clone, Serialize)]
pub struct SimilarityRecommendationResult {
    #[serde(flatten)]
    pub result: SimilarityResult,
    pub recommended_by: SimilarityBeatmap,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManiaSimilarityRecommendationResult {
    #[serde(flatten)]
    pub result: ManiaSimilarityResult,
    pub recommended_by: ManiaSimilarityBeatmap,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManiaSimilarityRecommendationGroup {
    pub key_count: u8,
    pub seed_count: usize,
    pub results: Vec<ManiaSimilarityRecommendationResult>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "ruleset", rename_all = "lowercase")]
pub enum SimilarityRecommendationResponse {
    Osu {
        kind: SimilarityRecommendationKind,
        seed_count: usize,
        skipped_seed_count: usize,
        results: Vec<SimilarityRecommendationResult>,
        dynamic_profiles: Vec<SimilaritySeedDynamicProfile>,
    },
    Mania {
        kind: SimilarityRecommendationKind,
        seed_count: usize,
        skipped_seed_count: usize,
        groups: Vec<ManiaSimilarityRecommendationGroup>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct SimilaritySeedDynamicProfile {
    pub seed_beatmap_id: u64,
    #[serde(flatten)]
    pub profile: DynamicWeightProfile,
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn unsupported_status_is_explicit_for_unimplemented_rulesets() {
        for ruleset in [Ruleset::Taiko, Ruleset::Fruits] {
            let status = SimilarityIndexStatus::unsupported(ruleset);
            assert_eq!(status.state, SimilarityIndexState::Unsupported);
            assert_eq!(status.ruleset, ruleset);
            assert!(status.message.contains("暂不支持"));
        }
    }

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
