// 从 types/osu.ts 按域拆分；types/osu.ts 是对外的 barrel，消费方导入路径不变。
export type Ruleset = "osu" | "taiko" | "fruits" | "mania";
export type ScoreCategory = "best" | "pinned" | "recent";
export type SkillDimension = "stamina" | "tenacity" | "agility" | "accuracy" | "precision" | "reaction" | "memory" | "reading";
export type OsuClient = "stable" | "lazer";

export interface Cached<T> {
  data: T;
  fetched_at: string;
  stale: boolean;
}

export interface CommandError {
  code: string;
  message: string;
  retry_after_seconds?: number;
  request_id?: string;
}

export type ThemeColor = "cyan" | "blue" | "violet" | "pink" | "orange" | "green";
export type ThemeMode = "dark" | "light";
