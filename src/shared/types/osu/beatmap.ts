// 从 types/osu.ts 按域拆分；types/osu.ts 是对外的 barrel，消费方导入路径不变。
import type { Ruleset } from "./common";

export interface Beatmap {
  id?: number;
  beatmapset_id?: number;
  difficulty_rating?: number;
  mode?: Ruleset;
  status?: string;
  total_length?: number;
  hit_length?: number;
  version?: string;
  accuracy?: number;
  ar?: number;
  bpm?: number;
  cs?: number;
  drain?: number;
  passcount?: number;
  playcount?: number;
  url?: string;
  [key: string]: any;
}

export interface Beatmapset {
  id?: number;
  artist?: string;
  artist_unicode?: string;
  title?: string;
  title_unicode?: string;
  creator?: string;
  covers?: {
    cover?: string;
    card?: string;
    list?: string;
    slimcover?: string;
    [key: string]: string | undefined;
  };
  [key: string]: any;
}

export interface Score {
  id?: number | null;
  user_id: number;
  accuracy: number;
  pp?: number | null;
  rank: string;
  total_score?: number | null;
  legacy_total_score?: number | null;
  max_combo?: number | null;
  ended_at?: string | null;
  created_at?: string | null;
  has_replay?: boolean | null;
  mods: Array<string | { acronym?: string; settings?: Record<string, unknown> }>;
  statistics: Record<string, number>;
  maximum_statistics?: Record<string, number> | null;
  beatmap?: Beatmap | null;
  beatmapset?: Beatmapset | null;
  weight?: { percentage?: number; pp?: number } | null;
  [key: string]: any;
}
