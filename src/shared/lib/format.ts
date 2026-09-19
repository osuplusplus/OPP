import type { Ruleset, Score } from "../types/osu";

/** All user-facing dates and times in OPP use China Standard Time (UTC+8). */
export const APP_TIME_ZONE = "Asia/Shanghai";

export const rulesetLabels: Record<Ruleset, string> = {
  osu: "osu!",
  taiko: "taiko",
  fruits: "catch",
  mania: "mania",
};

export function compactNumber(value?: number | null): string {
  if (value === null || value === undefined) return "—";
  return new Intl.NumberFormat("zh-CN", {
    notation: Math.abs(value) >= 10_000 ? "compact" : "standard",
    maximumFractionDigits: 2,
  }).format(value);
}

export function fullNumber(value?: number | null): string {
  if (typeof value !== "number" || !Number.isFinite(value)) return "—";
  return new Intl.NumberFormat("zh-CN").format(value);
}

export function fixedNumber(
  value?: number | null,
  digits = 0,
): string {
  return typeof value === "number" && Number.isFinite(value)
    ? value.toFixed(digits)
    : "—";
}

export function percent(value?: number | null, digits = 2): string {
  const formatted = fixedNumber(value, digits);
  return formatted === "—" ? formatted : `${formatted}%`;
}

export function duration(seconds?: number | null): string {
  if (seconds === null || seconds === undefined) return "—";
  const hours = seconds / 3600;
  return `${new Intl.NumberFormat("zh-CN", {
    maximumFractionDigits: hours < 10 ? 1 : 0,
  }).format(hours)} 小时`;
}

export function dateTime(value?: string | null): string {
  if (!value) return "—";
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) return value;
  return new Intl.DateTimeFormat("zh-CN", {
    dateStyle: "medium",
    timeStyle: "short",
    timeZone: APP_TIME_ZONE,
  }).format(parsed);
}

export function dateOnly(value?: string | null): string {
  if (!value) return "—";
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) return value;
  return new Intl.DateTimeFormat("zh-CN", { dateStyle: "medium", timeZone: APP_TIME_ZONE }).format(parsed);
}

export function scoreMods(score: Score): string[] {
  return score.mods
    .map((mod) => (typeof mod === "string" ? mod : mod.acronym))
    .filter((mod): mod is string => Boolean(mod));
}

export function scoreTotal(score: Score): number | null {
  return score.total_score ?? score.legacy_total_score ?? null;
}

export function rankTone(rank: string): string {
  if (rank === "XH" || rank === "SH") return "rank-silver";
  if (rank === "X" || rank === "S") return "rank-gold";
  if (rank === "A") return "rank-green";
  return "rank-neutral";
}

export function errorMessage(error: unknown): string {
  if (typeof error === "object" && error && "message" in error) {
    return String(error.message);
  }
  return error instanceof Error ? error.message : "暂时无法加载数据";
}
