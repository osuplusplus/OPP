import type { CareerCalendarDay, CareerStats } from "../../shared/types/osu";

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
      if (days[cursor].stats && days[cursor].status === "captured") { previous = days[cursor]; break; }
    }
    for (let cursor = index + 1; cursor < days.length; cursor += 1) {
      if (days[cursor].stats && days[cursor].status === "captured") { next = days[cursor]; break; }
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
      : left ?? right ?? null;
  }
  return result;
}

function dayDistance(left: string, right: string) {
  return Math.round((new Date(`${right}T00:00:00Z`).getTime() - new Date(`${left}T00:00:00Z`).getTime()) / 86_400_000);
}

function iso(value: Date) {
  return value.toISOString().slice(0, 10);
}
