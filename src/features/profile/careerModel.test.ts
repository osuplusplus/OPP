import { describe, expect, it } from "vitest";
import { activityDescription, activityLevel, buildPlayActivity, buildTrendData, careerToday, changePeriodView, currentPeriod, describeDelta, fillCareerDays, interpolateStats, monthDays, monthRange, periodRange, periodSummary, shiftPeriod, weekday } from "./careerModel";
import { careerFixtureDay, careerFixtureDays, careerFixtureStats, careerFixtureToday } from "./careerFixtures.test-data";

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

  it("navigates months and years with a full preceding month buffer", () => {
    expect(careerToday(new Date("2026-12-31T17:00:00Z"))).toBe("2027-01-01");
    const period = currentPeriod("2026-01-08");
    expect(shiftPeriod(period, -1)).toEqual({ view: "month", year: 2025, month: 11 });
    expect(periodRange(period)).toEqual({ start: "2026-01-01", end: "2026-01-31", queryStart: "2025-12-01" });
    expect(changePeriodView({ view: "year", year: 2025, month: 8 }, "month", careerFixtureToday).month).toBe(0);
    expect(changePeriodView({ view: "year", year: 2026, month: 8 }, "month", careerFixtureToday).month).toBe(8);
    expect(periodRange({ view: "year", year: 2024, month: 0 })).toEqual({ start: "2024-01-01", end: "2024-12-31", queryStart: "2023-12-01" });
    expect(fillCareerDays("2024-01-01", "2024-12-31", [])).toHaveLength(366);
  });

  it("distinguishes zero, gaps, partial data, failure and future without spreading plays", () => {
    const days = fillCareerDays("2026-09-30", "2026-10-10", careerFixtureDays);
    const result = buildPlayActivity(days, careerFixtureToday);
    const find = (date: string) => result.find((item) => item.day.date === date)!;
    expect(find("2026-09-30").state).toBe("unknown");
    expect(find("2026-10-02")).toMatchObject({ count: 0, level: 0, spanning: false, state: "recorded" });
    expect(find("2026-10-03")).toMatchObject({ count: null, state: "missing" });
    expect(find("2026-10-04")).toMatchObject({ count: 60, level: 3, spanning: true });
    expect(activityDescription(find("2026-10-04"))).toContain("自 2026-10-02 上次记录新增 60 次");
    expect(find("2026-10-06").state).toBe("failed");
    expect(activityDescription(find("2026-10-07"))).toContain("部分采集");
    expect(find("2026-10-08")).toMatchObject({ count: 115, level: 4 });
    expect(find("2026-10-09").state).toBe("future");
  });

  it("uses the same fixed intensity thresholds in monthly and yearly views", () => {
    expect([0, 1, 19, 20, 49, 50, 99, 100, 200].map(activityLevel)).toEqual([0, 1, 1, 2, 2, 3, 3, 4, 4]);
  });

  it("rejects unknown counts and resets, then starts comparing from the new baseline", () => {
    const days = [careerFixtureDay("2026-10-01"), careerFixtureDay("2026-10-02", { stats: careerFixtureStats({ play_count: null }) }),
      careerFixtureDay("2026-10-03", { stats: careerFixtureStats({ play_count: 200 }) }),
      careerFixtureDay("2026-10-04", { stats: careerFixtureStats({ play_count: 20 }) }),
      careerFixtureDay("2026-10-05", { stats: careerFixtureStats({ play_count: 25 }) })];
    expect(buildPlayActivity(days, careerFixtureToday).map((item) => item.count)).toEqual([null, null, null, null, 5]);
    expect(periodSummary(days, careerFixtureToday).plays).toBeNull();
  });

  it("retains score events from a partially captured day without profile statistics", () => {
    const day = careerFixtureDay("2026-10-05", { status: "partial", stats: null, added_scores: 2 });
    const [activity] = buildPlayActivity([day], careerFixtureToday);
    expect(activity.state).toBe("unknown");
    expect(activityDescription(activity)).toBe("次数无法确定 · 部分采集");
    expect(periodSummary([day], careerFixtureToday)).toMatchObject({ count: 0, plays: null, addedScores: 2 });
  });

  it("keeps estimates out of events, summaries and actual chart values", () => {
    const days = fillCareerDays("2026-10-01", "2026-10-08", careerFixtureDays);
    const actual = buildTrendData(days, "pp", false);
    const estimated = buildTrendData(days, "pp", true);
    expect(actual.find((point) => point.date === "2026-10-03")).toMatchObject({ value: null, estimate: null });
    expect(estimated.find((point) => point.date === "2026-10-03")).toMatchObject({ value: null, estimate: 5027.5, estimated: true });
    expect(estimated.find((point) => point.date === "2026-10-06")).toMatchObject({ value: null, estimate: null });
    const interpolated = interpolateStats(days);
    expect(interpolated.find((day) => day.date === "2026-10-03")).toMatchObject({ has_diff: false, added_scores: 0, added_medals: 0 });
    expect(periodSummary(interpolated, careerFixtureToday)).toMatchObject({ count: 6, plays: 250, addedScores: 2 });
    expect(buildPlayActivity(interpolated, careerFixtureToday).find((item) => item.day.date === "2026-10-03")!.state).toBe("missing");
  });

  it("does not invent estimates from a missing metric or beyond the endpoints", () => {
    const days = fillCareerDays("2026-10-01", "2026-10-06", [careerFixtureDay("2026-10-02", { stats: careerFixtureStats({ pp: null }) }), careerFixtureDay("2026-10-04")]);
    const result = interpolateStats(days);
    expect(result[0].stats).toBeNull();
    expect(result[2].stats?.pp).toBeNull();
    expect(result[5].stats).toBeNull();
  });

  it("has no comparisons for one snapshot, and explains rank and accuracy changes", () => {
    expect(periodSummary([careerFixtureDays[0]], careerFixtureToday)).toMatchObject({ count: 1, plays: null });
    expect(periodSummary([], careerFixtureToday)).toMatchObject({ first: null, last: null, count: 0, plays: null });
    expect(describeDelta("global_rank", 90, 100)).toEqual({ text: "上升 10 名", tone: "positive" });
    expect(describeDelta("global_rank", 110, 100)).toEqual({ text: "下降 10 名", tone: "negative" });
    expect(describeDelta("hit_accuracy", 98.4, 98).text).toBe("+0.40 个百分点");
    expect(describeDelta("pp", 100, null).text).toBe("暂无对比");
  });
});
