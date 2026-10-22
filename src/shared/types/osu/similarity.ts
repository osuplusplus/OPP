// 从 types/osu.ts 按域拆分；types/osu.ts 是对外的 barrel，消费方导入路径不变。
import type { Ruleset } from "./common";

export type SimilarityIndexState =
  | "unconfigured"
  | "missing"
  | "invalid"
  | "incompatible"
  | "unsupported"
  | "ready";

export type SimilarityRuleset = Extract<Ruleset, "osu" | "mania">;
export type ManiaKeyCount = 4 | 6 | 7;
export type ManiaGameMod = "NM" | "DT" | "HT";

export interface SimilarityIndexStatus {
  ruleset: Ruleset;
  state: SimilarityIndexState;
  directory: string | null;
  message: string;
  record_count: number | null;
  analyzer_version: number | null;
  normalization_version: number | null;
  algorithm_id: string | null;
  data_cutoff_at: number | null;
  supports_dynamic_weighting: boolean;
  records_by_key_count: Partial<Record<ManiaKeyCount, number>> | null;
}

export interface DifficultyFeatureVector {
  aim: number;
  speed: number;
  reading: number;
  slider: number;
  overlap: number;
}

export interface SimilarityBaseFeatures {
  bpm: number;
  ar: number;
  od: number;
  cs: number;
  hp: number;
  length_seconds: number;
  object_count: number;
  object_density: number;
  circle_ratio: number;
  slider_ratio: number;
  spinner_ratio: number;
  max_combo: number;
}

export interface SimilarityBaseWeights {
  bpm: number;
  ar: number;
  length_seconds: number;
  object_density: number;
  circle_ratio: number;
  slider_ratio: number;
}

export interface SimilarityFilters {
  min_star: number | null;
  max_star: number | null;
  min_ar: number | null;
  max_ar: number | null;
  min_cs: number | null;
  max_cs: number | null;
  min_od: number | null;
  max_od: number | null;
  min_bpm: number | null;
  max_bpm: number | null;
  min_length_seconds: number | null;
  max_length_seconds: number | null;
  min_object_density: number | null;
  max_object_density: number | null;
  min_circle_ratio: number | null;
  max_circle_ratio: number | null;
  min_slider_ratio: number | null;
  max_slider_ratio: number | null;
}

export type SimilaritySource =
  | { kind: "beatmap_id"; value: string }
  | { kind: "local_file"; path: string };

export interface OsuSimilarityQueryRequest {
  ruleset: "osu";
  source: SimilaritySource;
  weighting: SimilarityWeighting;
  filters: SimilarityFilters;
  result_limit: number;
}

export interface ManiaSimilarityQueryRequest {
  ruleset: "mania";
  source: SimilaritySource;
  result_limit: number;
  target_mod: ManiaGameMod;
  candidate_mods: ManiaGameMod[];
}

export type SimilarityQueryRequest =
  | OsuSimilarityQueryRequest
  | ManiaSimilarityQueryRequest;

export type SimilarityWeighting =
  | {
      mode: "dynamic";
      lower_sections: number;
      upper_sections: number;
    }
  | {
      mode: "manual";
      difficulty_weights: DifficultyFeatureVector;
      parameter_weight: number;
    };

export interface SimilarityParameterVector {
  ar: number;
  cs: number;
  od: number;
}

export interface SimilarityDynamicWeightProfile {
  seed_beatmap_id?: number | null;
  target_star_rating: number;
  candidate_min_section: number;
  candidate_max_section: number;
  stats_min_section: number;
  stats_max_section: number;
  sample_count: number;
  mean: DifficultyFeatureVector;
  stddev: DifficultyFeatureVector;
  delta: DifficultyFeatureVector;
  z_score: DifficultyFeatureVector;
  weights: DifficultyFeatureVector;
  parameter_mean: SimilarityParameterVector;
  parameter_stddev: SimilarityParameterVector;
  parameter_delta: SimilarityParameterVector;
  parameter_z_score: SimilarityParameterVector;
  parameter_group_z_score: number;
  parameter_weight: number;
  fallback_reason: string | null;
}

export interface SimilarityBeatmap {
  ruleset: "osu";
  beatmap_id: number;
  beatmapset_id: number;
  artist: string;
  title: string;
  version: string;
  creator: string;
  online_url: string;
  star_rating: number | null;
  difficulty: DifficultyFeatureVector;
  base: SimilarityBaseFeatures;
}

export interface SimilarityTarget extends SimilarityBeatmap {
  source: "index" | "online" | "local_file";
  analyzer_version: number;
  normalization_version: number;
}

export interface SimilarityResult extends SimilarityBeatmap {
  final_distance: number;
  difficulty_distance: number;
  base_distance: number;
}

export interface OsuSimilarityQueryResponse {
  ruleset: "osu";
  target: SimilarityTarget;
  results: SimilarityResult[];
  dynamic_profile: SimilarityDynamicWeightProfile | null;
}

export interface ManiaDifficultyVector {
  speed: number;
  hand_stream: number;
  jack: number;
  chordjack: number;
  technical: number;
  stamina: number;
  long_note: number;
  course: number;
}

export interface ManiaStyleVector {
  stream: number;
  chordstream: number;
  jacks: number;
  coordination: number;
  density: number;
  wildcard: number;
  chord_rate: number;
  large_chord_rate: number;
  rotation_rate: number;
  anchor_rate: number;
  rhythm_entropy: number;
  transition_entropy: number;
  ln_note_ratio: number;
  hold_occupancy: number;
  hybrid_row_ratio: number;
  peak_to_sustain_gap: number;
}

export interface ManiaBaseFeatures {
  bpm: number;
  length_seconds: number;
  active_length_seconds: number;
  note_count: number;
  row_count: number;
  avg_nps: number;
  peak_nps: number;
  break_density: number;
  sv_change_rate: number;
}

export type ManiaModeFamily = "rc" | "hb" | "mix" | "ln";
export type ManiaPattern =
  | "stream"
  | "chordstream"
  | "jacks"
  | "coordination"
  | "density"
  | "wildcard";

export interface ManiaPatternBar {
  pattern: string;
  amount: number;
  relative: number;
  specific_types: [string, number][];
}

export interface ManiaPatternView {
  category: string;
  mode_tag: string;
  coverage: number[];
  bars: ManiaPatternBar[];
  subtypes: [string, number][];
  ln_note_ratio: number;
  intensity: number[];
  temporal: number[];
  duration_seconds: number;
  sv_amount: number;
}

export interface ManiaSimilarityBeatmap {
  ruleset: "mania";
  beatmap_id: number;
  beatmapset_id: number;
  artist: string;
  title: string;
  version: string;
  creator: string;
  online_url: string;
  key_count: ManiaKeyCount;
  family: ManiaModeFamily;
  pattern: ManiaPattern;
  difficulty: ManiaDifficultyVector;
  style: ManiaStyleVector;
  base: ManiaBaseFeatures;
  difficulty_percentile: number;
  difficulty_band: number;
  game_mod: ManiaGameMod;
  pattern_view: ManiaPatternView | null;
}

export interface ManiaSimilarityTarget extends ManiaSimilarityBeatmap {
  source: "index" | "online" | "local_file";
  analyzer_version: number;
  normalization_version: number;
}

export interface ManiaDistanceComponents {
  skill: number;
  pattern: number;
  structure: number;
  difficulty: number;
  context: number;
}

export interface ManiaSimilarityResult extends ManiaSimilarityBeatmap {
  final_distance: number;
  distance_components: ManiaDistanceComponents;
}

export interface ManiaSimilarityQueryResponse {
  ruleset: "mania";
  target: ManiaSimilarityTarget;
  results: ManiaSimilarityResult[];
}

export type SimilarityQueryResponse =
  | OsuSimilarityQueryResponse
  | ManiaSimilarityQueryResponse;

export type SimilarityRecommendationKind = "recent" | "best";

export interface OsuSimilarityRecommendationRequest {
  ruleset: "osu";
  kind: SimilarityRecommendationKind;
  weighting: SimilarityWeighting;
  filters: SimilarityFilters;
  result_limit: number;
  seed_limit?: number;
  excluded_beatmap_ids?: number[];
}

export interface ManiaSimilarityRecommendationRequest {
  ruleset: "mania";
  kind: SimilarityRecommendationKind;
  result_limit: number;
  seed_limit?: number;
  excluded_beatmap_ids?: number[];
  candidate_mods: ManiaGameMod[];
}

export type SimilarityRecommendationRequest =
  | OsuSimilarityRecommendationRequest
  | ManiaSimilarityRecommendationRequest;

export interface SimilarityRecommendationResult extends SimilarityResult {
  recommended_by: SimilarityBeatmap;
}

export interface OsuSimilarityRecommendationResponse {
  ruleset: "osu";
  kind: SimilarityRecommendationKind;
  seed_count: number;
  skipped_seed_count: number;
  results: SimilarityRecommendationResult[];
  dynamic_profiles: SimilarityDynamicWeightProfile[];
}

export interface ManiaSimilarityRecommendationResult extends ManiaSimilarityResult {
  recommended_by: ManiaSimilarityBeatmap;
}

export interface ManiaSimilarityRecommendationGroup {
  key_count: ManiaKeyCount;
  seed_count: number;
  results: ManiaSimilarityRecommendationResult[];
}

export interface ManiaSimilarityRecommendationResponse {
  ruleset: "mania";
  kind: SimilarityRecommendationKind;
  seed_count: number;
  skipped_seed_count: number;
  groups: ManiaSimilarityRecommendationGroup[];
}

export type SimilarityRecommendationResponse =
  | OsuSimilarityRecommendationResponse
  | ManiaSimilarityRecommendationResponse;

export type AnySimilarityBeatmap = SimilarityBeatmap | ManiaSimilarityBeatmap;
export type AnySimilarityResult = SimilarityResult | ManiaSimilarityResult;
export type AnySimilarityRecommendationResult =
  | SimilarityRecommendationResult
  | ManiaSimilarityRecommendationResult;
