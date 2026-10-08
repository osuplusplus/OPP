import type { CareerCalendarDay, CareerStats } from "../../shared/types/osu";
import { APP_TIME_ZONE, duration, fullNumber, percent } from "../../shared/lib/format";

export const careerStatLabels: Array<[keyof CareerStats, string]> = [
  ["pp", "PP"], ["global_rank", "全球排名"], ["hit_accuracy", "准确率"], ["play_count", "累计游玩"],
  ["country_rank", "地区排名"], ["ranked_score", "排名分"], ["total_score", "总分"],
  ["play_time", "游玩时长"], ["total_hits", "总命中数"], ["maximum_combo", "最大连击"],
  ["level", "等级"], ["level_progress", "等级进度"],
];

export function formatCareerStat(key: keyof CareerStats, raw: number | null | undefined) {
  const value = finiteValue(raw);
  if (value == null) return "—";
  if (key === "pp") return value.toFixed(2);
  if (key === "global_rank" || key === "country_rank") return `#${fullNumber(Math.round(value))}`;
  if (key === "hit_accuracy" || key === "level_progress") return percent(value);
  if (key === "play_time") return duration(value);
  return `${fullNumber(value)}${key === "maximum_combo" ? "x" : ""}`;
}

export type CareerPeriod = { view: "month" | "year"; year: number; month: number };
export type ChartMetric = "pp" | "global_rank" | "hit_accuracy" | "play_count";
export type PlayActivity = {
  day: CareerCalendarDay;
  count: number | null;
  level: number;
  state: "recorded" | "unknown" | "missing" | "failed" | "future";
  spanning: boolean;
  previousDate: string | null;
};

export function careerToday(now = new Date()) {
  const parts = new Intl.DateTimeFormat("en-US", { timeZone: APP_TIME_ZONE, year: "numeric", month: "2-digit", day: "2-digit" }).formatToParts(now);
  const part = (type: Intl.DateTimeFormatPartTypes) => parts.find((item) => item.type === type)!.value;
  return `${part("year")}-${part("month")}-${part("day")}`;
}

export function currentPeriod(today: string): CareerPeriod {
  return { view: "month", year: Number(today.slice(0, 4)), month: Number(today.slice(5, 7)) - 1 };
}

export function changePeriodView(period: CareerPeriod, view: CareerPeriod["view"], today: string): CareerPeriod {
  return { ...period, view, month: view === "month" && period.view === "year" && period.year !== Number(today.slice(0, 4)) ? 0 : period.month };
}

export function shiftPeriod(period: CareerPeriod, amount: number): CareerPeriod {
  if (period.view === "year") return { ...period, year: period.year + amount };
  const date = new Date(Date.UTC(period.year, period.month + amount, 1));
  return { ...period, year: date.getUTCFullYear(), month: date.getUTCMonth() };
}

export function periodRange(period: CareerPeriod) {
  const range = period.view === "month" ? monthRange(period.year, period.month) : { start: `${period.year}-01-01`, end: `${period.year}-12-31` };
  const buffer = new Date(`${range.start}T00:00:00Z`);
  buffer.setUTCMonth(buffer.getUTCMonth() - 1);
  return { ...range, queryStart: iso(buffer) };
}

export function rangeDays(start: string, end: string) {
  const result: string[] = [];
  for (let cursor = start; cursor <= end; cursor = addDays(cursor, 1)) result.push(cursor);
  return result;
}

export function fillCareerDays(start: string, end: string, source: CareerCalendarDay[]): CareerCalendarDay[] {
  const byDate = new Map(source.map((day) => [day.date, day]));
  return rangeDays(start, end).map((date) => byDate.get(date) ?? {
    date, status: "missing", captured_at: null, stats: null, error: null, has_diff: false,
    added_scores: 0, removed_scores: 0, changed_scores: 0, added_medals: 0, added_replays: 0, added_screenshots: 0,
  });
}

export function recordedDay(day: CareerCalendarDay) {
  return day.status === "captured" || day.status === "partial";
}

export function realSnapshot(day: CareerCalendarDay) {
  return recordedDay(day) && day.stats != null;
}

export function finiteValue(value: number | null | undefined): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

export function activityLevel(count: number) {
  return count === 0 ? 0 : count < 20 ? 1 : count < 50 ? 2 : count < 100 ? 3 : 4;
}

export function buildPlayActivity(days: CareerCalendarDay[], today: string): PlayActivity[] {
  let previous: CareerCalendarDay | undefined;
  return [...days].sort((a, b) => a.date.localeCompare(b.date)).map((day) => {
    const base = { day, count: null, level: 0, spanning: false, previousDate: previous?.date ?? null };
    if (day.date > today) return { ...base, state: "future" };
    if (!realSnapshot(day)) return { ...base, state: day.status === "unavailable" ? "failed" : day.status === "partial" || day.status === "captured" ? "unknown" : "missing" };
    const before = finiteValue(previous?.stats?.play_count);
    const current = finiteValue(day.stats?.play_count);
    const delta = before != null && current != null ? current - before : null;
    const spanning = previous != null && dayDistance(previous.date, day.date) > 1;
    previous = day;
    if (delta == null || delta < 0) return { ...base, state: "unknown", spanning };
    return { ...base, count: delta, level: activityLevel(delta), state: "recorded", spanning };
  });
}

export function activityDescription(activity: PlayActivity) {
  const { state, count, spanning, previousDate, day } = activity;
  const partial = day.status === "partial" ? " · 部分采集" : "";
  if (state === "future") return "未来日期";
  if (state === "missing") return "未采集";
  if (state === "failed") return "采集失败";
  if (state === "unknown") return `次数无法确定${partial}`;
  return `${spanning ? `自 ${previousDate} 上次记录新增` : "新增游玩"} ${fullNumber(count)} 次${partial}`;
}

export function periodSummary(days: CareerCalendarDay[], today: string) {
  const snapshots = days.filter((day) => day.date <= today && realSnapshot(day));
  const first = snapshots[0] ?? null;
  const last = snapshots[snapshots.length - 1] ?? null;
  const plays = snapshots.length > 1 ? statDelta(last?.stats?.play_count, first?.stats?.play_count) : null;
  // Counter resets or unknown intermediate samples cannot establish a period total.
  const reliablePlays = snapshots.length > 1 && snapshots.every((day, index) => {
    const value = finiteValue(day.stats?.play_count);
    const before = index ? finiteValue(snapshots[index - 1].stats?.play_count) : value;
    return value != null && before != null && value >= before;
  });
  const addedScores = days.filter((day) => day.date <= today && recordedDay(day)).reduce((sum, day) => sum + day.added_scores, 0);
  return { first, last, count: snapshots.length, plays: reliablePlays ? plays : null, addedScores };
}

export function statDelta(current: number | null | undefined, before: number | null | undefined) {
  const left = finiteValue(current);
  const right = finiteValue(before);
  return left != null && right != null ? left - right : null;
}

export function describeDelta(key: keyof CareerStats, current: number | null | undefined, before: number | null | undefined) {
  const delta = statDelta(current, before);
  if (delta == null) return { text: "暂无对比", tone: "neutral" };
  if (delta === 0) return { text: "无变化", tone: "neutral" };
  const rank = key === "global_rank" || key === "country_rank";
  const positive = rank ? delta < 0 : delta > 0;
  const text = rank ? `${positive ? "上升" : "下降"} ${fullNumber(Math.abs(delta))} 名`
    : `${delta > 0 ? "+" : ""}${key === "pp" || key === "hit_accuracy" ? delta.toFixed(2) : fullNumber(delta)}${key === "hit_accuracy" ? " 个百分点" : key === "pp" ? " PP" : ""}`;
  return { text, tone: positive ? "positive" : "negative" };
}

export function buildTrendData(days: CareerCalendarDay[], metric: ChartMetric, estimates: boolean) {
  const source = estimates ? interpolateStats(days) : days;
  return source.map((day, index) => {
    const value = finiteValue(day.stats?.[metric]);
    const real = realSnapshot(day);
    const estimated = day.status === "interpolated";
    const touchesEstimate = source[index - 1]?.status === "interpolated" || source[index + 1]?.status === "interpolated";
    return { date: day.date, value: real ? value : null, estimate: estimated || (real && touchesEstimate) ? value : null, estimated, status: day.status };
  });
}

export function monthRange(year: number, month: number) {
  const start = new Date(Date.UTC(year, month, 1));
  const end = new Date(Date.UTC(year, month + 1, 0));
  return { start: iso(start), end: iso(end) };
}

export function monthDays(year: number, month: number): string[] {
  const { start, end } = monthRange(year, month);
  const result: string[] = [];
  for (let cursor = start; cursor <= end; cursor = addDays(cursor, 1)) result.push(cursor);
  return result;
}

export function addDays(value: string, amount: number) {
  const date = new Date(`${value}T00:00:00Z`);
  date.setUTCDate(date.getUTCDate() + amount);
  return iso(date);
}

export function weekday(value: string) {
  return new Date(`${value}T00:00:00Z`).getUTCDay();
}

export function interpolateStats(days: CareerCalendarDay[]): CareerCalendarDay[] {
  return days.map((day, index) => {
    if (day.status !== "missing" || day.stats) return day;
    let previous: CareerCalendarDay | undefined;
    let next: CareerCalendarDay | undefined;
    for (let cursor = index - 1; cursor >= 0; cursor -= 1) {
      if (realSnapshot(days[cursor])) { previous = days[cursor]; break; }
    }
    for (let cursor = index + 1; cursor < days.length; cursor += 1) {
      if (realSnapshot(days[cursor])) { next = days[cursor]; break; }
    }
    if (!previous || !next) return day;
    const total = dayDistance(previous.date, next.date);
    const offset = dayDistance(previous.date, day.date);
    return { ...day, status: "interpolated", stats: interpolate(previous.stats!, next.stats!, offset / total) };
  });
}

function interpolate(previous: CareerStats, next: CareerStats, ratio: number): CareerStats {
  const result = {} as CareerStats;
  for (const key of Object.keys(previous) as Array<keyof CareerStats>) {
    const left = previous[key];
    const right = next[key];
    result[key] = typeof left === "number" && typeof right === "number"
      ? left + (right - left) * ratio
      : null;
  }
  return result;
}

function dayDistance(left: string, right: string) {
  return Math.round((new Date(`${right}T00:00:00Z`).getTime() - new Date(`${left}T00:00:00Z`).getTime()) / 86_400_000);
}

function iso(value: Date) {
  return value.toISOString().slice(0, 10);
}
