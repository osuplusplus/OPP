import type { CareerCalendarDay, CareerDayDetail, CareerStats, OwnProfile } from "../../shared/types/osu";

export const careerFixtureToday = "2026-10-08";
export const careerFixtureProfile: OwnProfile = {
  id: 1, username: "Player", avatar_url: "", country_code: "CN", is_active: true, is_online: false, is_supporter: false,
};
export function careerFixtureStats(overrides: Partial<CareerStats> = {}): CareerStats {
  return { pp: 5000, global_rank: 10000, country_rank: 500, ranked_score: 90000000, total_score: 100000000,
    hit_accuracy: 98, play_count: 100, play_time: 7200, total_hits: 50000, maximum_combo: 1000, level: 90, level_progress: 50, ...overrides };
}
export function careerFixtureDay(date: string, overrides: Partial<CareerCalendarDay> = {}): CareerCalendarDay {
  return { date, status: "captured", captured_at: `${date}T10:00:00Z`, stats: careerFixtureStats(), error: null, has_diff: false,
    added_scores: 0, removed_scores: 0, changed_scores: 0, added_medals: 0, added_replays: 0, added_screenshots: 0, ...overrides };
}
export const careerFixtureDays = [
  careerFixtureDay("2026-09-30", { stats: careerFixtureStats({ pp: 4980, play_count: 90 }) }),
  careerFixtureDay("2026-10-01"),
  careerFixtureDay("2026-10-02", { stats: careerFixtureStats({ pp: 5010 }) }),
  careerFixtureDay("2026-10-04", { stats: careerFixtureStats({ pp: 5045, global_rank: 9850, play_count: 160 }), added_scores: 1, has_diff: true }),
  careerFixtureDay("2026-10-05", { stats: careerFixtureStats({ pp: 5090, global_rank: 9750, play_count: 180 }) }),
  careerFixtureDay("2026-10-06", { status: "unavailable", stats: null, error: "网络暂时不可用" }),
  careerFixtureDay("2026-10-07", { status: "partial", stats: careerFixtureStats({ pp: 5120, global_rank: 9700, play_count: 235 }), error: "媒体未采集成功" }),
  careerFixtureDay("2026-10-08", { stats: careerFixtureStats({ pp: 5180, global_rank: 9500, play_count: 350, hit_accuracy: 98.4 }), added_scores: 1, has_diff: true }),
];
export function careerFixtureDetail(date: string): CareerDayDetail {
  const index = careerFixtureDays.findIndex((item) => item.date === date);
  const day = careerFixtureDays[index];
  const previous = careerFixtureDays.slice(0, index).reverse().find((item) => item.stats);
  return { date, ruleset: "osu", status: day?.status ?? "missing", captured_at: day?.captured_at ?? null,
    stats: day?.stats ?? null, previous_stats: previous?.stats ?? null, error: day?.error ?? null, medal_events: [], media_events: [],
    score_diffs: day?.added_scores ? [{ kind: "added", key: `score-${date}`, before_position: null, after_position: 12,
      score: { id: 42, user_id: 1, accuracy: .99, rank: "S", pp: 320.5, mods: ["HD"], statistics: {}, beatmap: { id: 123, version: "Insane" }, beatmapset: { title: "Signal Garden", artist: "Preview Artist" } } }] : [] };
}
