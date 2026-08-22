import { describe, expect, it } from "vitest";

import { defaultSimilarityFilters, createSimilarityRequest } from "./defaults";
import {
  matchesCandidateFilters,
  resolveSimilarityWeighting,
} from "./viewModel";

const base = {
  ar: 9,
  cs: 4,
  od: 8.5,
  hp: 6,
  bpm: 180,
  length_seconds: 120,
  object_count: 600,
  object_density: 5,
  circle_ratio: 0.6,
  slider_ratio: 0.4,
  spinner_ratio: 0,
  max_combo: 900,
};

describe("similarity view model", () => {
  it("applies candidate bounds without rejecting unset filters", () => {
    expect(matchesCandidateFilters(base, 6.2, defaultSimilarityFilters)).toBe(true);
    expect(matchesCandidateFilters(base, 6.2, { ...defaultSimilarityFilters, min_bpm: 181 })).toBe(false);
    expect(matchesCandidateFilters(base, null, { ...defaultSimilarityFilters, min_star: 1 })).toBe(false);
  });

  it("falls back to saved manual weights when dynamic statistics are unavailable", () => {
    const request = createSimilarityRequest({ kind: "beatmap_id", value: "1" });
    const weighting = resolveSimilarityWeighting(request, {
      advanced_enabled: true,
      mode: "dynamic",
      lower_sections: 4,
      upper_sections: 4,
      manual_weights: { aim: 0.5, speed: 1, reading: 1, slider: 0, overlap: 0, parameters: 0.75 },
      results_per_page: 5,
    });

    expect(weighting).toMatchObject({ mode: "manual", parameter_weight: 0.75 });
  });
});
