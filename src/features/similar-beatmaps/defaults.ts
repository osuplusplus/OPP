import type {
  DifficultyFeatureVector,
  SimilarityPreferences,
  SimilarityFilters,
  ManiaSimilarityQueryRequest,
  OsuSimilarityQueryRequest,
  SimilaritySource,
} from "../../shared/types/osu";

export const defaultDifficultyWeights: DifficultyFeatureVector = {
  aim: 1,
  speed: 2,
  reading: 2,
  slider: 0,
  overlap: 0.25,
};

export const defaultDynamicWeighting = {
  mode: "dynamic" as const,
  lower_sections: 4,
  upper_sections: 4,
};

export const defaultSimilarityPreferences: SimilarityPreferences = {
  advanced_enabled: false,
  mode: "manual",
  lower_sections: 4,
  upper_sections: 4,
  manual_weights: { ...defaultDifficultyWeights, parameters: 1 },
  results_per_page: 5,
};

export function manualWeightingFromPreferences(preferences = defaultSimilarityPreferences) {
  const { parameters, ...difficulty_weights } = preferences.manual_weights;
  return { mode: "manual" as const, difficulty_weights, parameter_weight: parameters };
}

export const defaultSimilarityFilters: SimilarityFilters = {
  min_star: null,
  max_star: null,
  min_ar: null,
  max_ar: null,
  min_cs: null,
  max_cs: null,
  min_od: null,
  max_od: null,
  min_bpm: null,
  max_bpm: null,
  min_length_seconds: null,
  max_length_seconds: null,
  min_object_density: null,
  max_object_density: null,
  min_circle_ratio: null,
  max_circle_ratio: null,
  min_slider_ratio: null,
  max_slider_ratio: null,
};

export function createSimilarityRequest(
  source: SimilaritySource,
): OsuSimilarityQueryRequest {
  return {
    ruleset: "osu",
    source,
    weighting: manualWeightingFromPreferences(),
    filters: { ...defaultSimilarityFilters },
    result_limit: 50,
  };
}

export function createManiaSimilarityRequest(
  source: SimilaritySource,
): ManiaSimilarityQueryRequest {
  return {
    ruleset: "mania",
    source,
    result_limit: 50,
    target_mod: "NM",
    candidate_mods: ["NM"],
  };
}
