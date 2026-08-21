import { beforeEach, describe, expect, it } from "vitest";

import type { ManiaSimilarityResult, SimilarityResult } from "../../shared/types/osu";
import {
  getTodayRecommendationHistory,
  getTodayRecommendedBeatmapIds,
  recordDisplayedRecommendationBatch,
} from "./recommendationHistory";

function today() {
  const date = new Date();
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
}

const standardResult: SimilarityResult = {
  ruleset: "osu",
  beatmap_id: 42,
  beatmapset_id: 4,
  artist: "Standard",
  title: "Candidate",
  version: "Insane",
  creator: "Mapper",
  online_url: "https://osu.ppy.sh/beatmaps/42",
  star_rating: 5.8,
  difficulty: { aim: 0.5, speed: 0.5, reading: 0.5, slider: 0.5, overlap: 0.5 },
  base: { bpm: 180, ar: 9, od: 8, cs: 4, hp: 6, length_seconds: 120, object_count: 500, object_density: 4, circle_ratio: 0.6, slider_ratio: 0.38, spinner_ratio: 0.02, max_combo: 700 },
  final_distance: 0.1,
  difficulty_distance: 0.1,
  base_distance: 0.1,
};

const maniaResult: ManiaSimilarityResult = {
  ruleset: "mania",
  beatmap_id: 42,
  beatmapset_id: 5,
  artist: "Mania",
  title: "Candidate",
  version: "6K Another",
  creator: "Mapper",
  online_url: "https://osu.ppy.sh/beatmaps/42",
  key_count: 6,
  family: "hb",
  pattern: "coordination",
  difficulty: { speed: 0.6, hand_stream: 0.7, jack: 0.3, chordjack: 0.4, technical: 0.5, stamina: 0.6, long_note: 0.1, course: 0.4 },
  style: { stream: 0.5, chordstream: 0.4, jacks: 0.2, coordination: 0.7, density: 0.5, wildcard: 0.1, chord_rate: 0.3, large_chord_rate: 0.1, rotation_rate: 0.4, anchor_rate: 0.2, rhythm_entropy: 0.5, transition_entropy: 0.5, ln_note_ratio: 0.05, hold_occupancy: 0.04, hybrid_row_ratio: 0.02, peak_to_sustain_gap: 0.3 },
  base: { bpm: 180, length_seconds: 120, active_length_seconds: 110, note_count: 800, row_count: 700, avg_nps: 7, peak_nps: 12, break_density: 0.1, sv_change_rate: 0 },
  difficulty_percentile: 0.75,
  difficulty_band: 7,
  final_distance: 0.08,
  distance_components: { skill: 0.1, pattern: 0.1, structure: 0.1, difficulty: 0.1, context: 0.1 },
};

beforeEach(() => localStorage.clear());

describe("recommendation history", () => {
  it("migrates v1 entries to osu!standard", () => {
    const legacyResult: Partial<SimilarityResult> = { ...standardResult };
    delete legacyResult.ruleset;
    localStorage.setItem("opp.similarity-recommendation-history.v1", JSON.stringify({
      day: today(),
      entries: [{ displayed_at: new Date().toISOString(), result: legacyResult }],
    }));

    const history = getTodayRecommendationHistory("osu");
    expect(history).toHaveLength(1);
    expect(history[0]).toMatchObject({ ruleset: "osu", key_count: null });
    expect(history[0].result.ruleset).toBe("osu");
    expect(getTodayRecommendationHistory("mania")).toEqual([]);
    expect(localStorage.getItem("opp.similarity-recommendation-history.v2")).toContain('"version":2');
  });

  it("does not let the same Beatmap ID collide across rulesets", () => {
    recordDisplayedRecommendationBatch([standardResult], "osu", 1);
    recordDisplayedRecommendationBatch([maniaResult], "mania", 1);

    expect(getTodayRecommendedBeatmapIds("osu")).toEqual(new Set([42]));
    expect(getTodayRecommendedBeatmapIds("mania")).toEqual(new Set([42]));
    expect(getTodayRecommendationHistory("osu")).toHaveLength(1);
    expect(getTodayRecommendationHistory("mania")).toHaveLength(1);
  });

  it("persists the Mania key count in v2", () => {
    recordDisplayedRecommendationBatch([maniaResult], "mania", 1);
    expect(getTodayRecommendationHistory("mania")[0]).toMatchObject({ ruleset: "mania", key_count: 6 });
    expect(localStorage.getItem("opp.similarity-recommendation-history.v2")).toContain('"key_count":6');
  });
});
