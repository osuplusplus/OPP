// 从 types/osu.ts 按域拆分；types/osu.ts 是对外的 barrel，消费方导入路径不变。
import type { OsuClient } from "./common";
import type { LocalBeatmapSummary } from "./localAnalysis";
import type { Completeness } from "./localResources";

export interface TrainerRequest {
  client: OsuClient;
  resource_id: string;
  rate: number;
  ar: number;
  od: number;
  cs: number;
  hp: number;
  min_bpm: number | null;
  max_bpm: number | null;
  start_time_ms: number | null;
  end_time_ms: number | null;
}

export interface TrainerResult {
  directory: string;
  beatmap_path: string;
  included_objects: number;
}

export interface ViewTrainerRequest {
  client: OsuClient;
  resourceId: string;
  rate: number;
  bpmLocked: boolean;
  targetBpm: number | null;
  ar: number;
  od: number;
  cs: number;
  hp: number;
  scaleAr: boolean;
  scaleOd: boolean;
  lockAr: boolean;
  lockOd: boolean;
  lockCs: boolean;
  lockHp: boolean;
  noSpinners: boolean;
  changePitch: boolean;
  previewOnly: boolean;
  minBpm: number | null;
  maxBpm: number | null;
  startTimeMs: number | null;
  endTimeMs: number | null;
}

export interface ViewTrainerTimeline {
  durationMs: number;
  bpmSegments: Array<[number, number]>;
  ar: number;
  od: number;
  cs: number;
  hp: number;
  objectCount: number;
  mode: number;
  primaryBpm: number | null;
  strainSeries: Array<{ key: string; values: number[] }>;
  strainSectionStartTimeMs: number;
  strainSectionLengthMs: number;
}

export interface LocalBeatmapSetSummary {
  set_key: string;
  completeness: Completeness;
  grouping_inferred: boolean;
  beatmap_set_id: number | null;
  title: string;
  title_unicode: string;
  artist: string;
  artist_unicode: string;
  creators: string[];
  min_stars: number | null;
  max_stars: number | null;
  bpm: number;
  length_ms: number;
  object_count: number;
  modified_at: string | null;
  background_resource_id: string | null;
  difficulties: LocalBeatmapSummary[];
}

export interface LocalBeatmapAudioPayload {
  mime_type: string;
  bytes_base64: string;
  preview_time_ms: number;
}
