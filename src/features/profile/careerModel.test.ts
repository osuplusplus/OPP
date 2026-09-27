import { describe, expect, it } from "vitest";
import { interpolateStats, monthDays, monthRange, weekday } from "./careerModel";

describe("careerModel", () => {
  it("builds a timezone independent month grid", () => {
    expect(monthRange(2026, 1)).toEqual({ start: "2026-02-01", end: "2026-02-28" });
    expect(monthDays(2026, 1)).toHaveLength(28);
    expect(weekday("2026-02-01")).toBe(0);
  });

  it("interpolates only between real snapshots", () => {
    const days = [
      { date: "2026-01-01", status: "captured", captured_at: null, stats: { pp: 100, global_rank: 100, country_rank: null, ranked_score: null, total_score: 1000, hit_accuracy: 98, play_count: 10, play_time: null, total_hits: null, maximum_combo: null, level: null, level_progress: null }, error: null, has_diff: false, added_scores: 0, removed_scores: 0, changed_scores: 0, added_medals: 0, added_replays: 0, added_screenshots: 0 },
      { date: "2026-01-02", status: "missing", captured_at: null, stats: null, error: null, has_diff: false, added_scores: 0, removed_scores: 0, changed_scores: 0, added_medals: 0, added_replays: 0, added_screenshots: 0 },
      { date: "2026-01-03", status: "captured", captured_at: null, stats: { pp: 200, global_rank: 80, country_rank: null, ranked_score: null, total_score: 3000, hit_accuracy: 99, play_count: 20, play_time: null, total_hits: null, maximum_combo: null, level: null, level_progress: null }, error: null, has_diff: false, added_scores: 0, removed_scores: 0, changed_scores: 0, added_medals: 0, added_replays: 0, added_screenshots: 0 },
    ];
    const result = interpolateStats(days);
    expect(result[1].status).toBe("interpolated");
    expect(result[1].stats?.pp).toBe(150);
    expect(result[1].stats?.global_rank).toBe(90);
  });
});
