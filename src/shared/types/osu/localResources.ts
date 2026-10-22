// 从 types/osu.ts 按域拆分；types/osu.ts 是对外的 barrel，消费方导入路径不变。
import type { OsuClient } from "./common";

export interface LocalBeatmapPresence {
  beatmap_id: number;
  status: "present" | "missing" | "unknown";
  clients: OsuClient[];
}
export interface LocalLibraryStorageStatus {
  client: OsuClient;
  storage: "database" | "json" | "unscanned";
  revision: string | null;
  entry_count: number;
  beatmap_count: number;
  error: string | null;
}
export type Completeness = "complete" | "partial";
export type CapabilityLevel = "full" | "partial" | "unavailable";
export type BeatmapDownloadProvider = "sayobot" | "hinai" | "catboy" | "nerinyan";

export interface PlatformCapabilities {
  os: "windows" | "linux";
  display_gamma: boolean;
  file_association: boolean;
}

export interface LazerDiskUsage {
  path: string;
  total_size: number;
  unique_size: number;
  file_count: number;
}

export interface LazerDedupeProgress {
  phase: string;
  processed: number;
  total: number;
  percent: number;
}

export interface LazerDedupeFailure {
  path: string;
  message: string;
}

export interface LazerDedupeResult {
  dry_run: boolean;
  cancelled: boolean;
  lazer_files_root: string;
  stable_roots: string[];
  lazer_file_count: number;
  lazer_total_size: number;
  already_linked_count: number;
  already_linked_size: number;
  hashed_stable_count: number;
  candidate_count: number;
  reclaimable_size: number;
  linked_count: number;
  linked_size: number;
  skipped_cross_volume_count: number;
  skipped_cross_volume_size: number;
  failed_count: number;
  failed: LazerDedupeFailure[];
}

export interface VideoFileEntry {
  path: string;
  size: number;
}

export interface VideoScanResult {
  songs_root: string;
  video_count: number;
  video_size: number;
  pending_count: number;
  pending_size: number;
  pending: VideoFileEntry[];
  pending_truncated: boolean;
  already_flv_count: number;
  empty_count: number;
  other_count: number;
}

export interface VideoFixProgress {
  phase: "scan" | "copy" | "transcode";
  processed: number;
  total: number;
  percent: number;
  current: string | null;
}

export interface VideoFixFailure {
  path: string;
  message: string;
}

export interface VideoFixResult {
  songs_root: string;
  cancelled: boolean;
  fixed_count: number;
  fixed_size: number;
  remuxed_count: number;
  transcoded_count: number;
  skipped_count: number;
  failed_count: number;
  failed: VideoFixFailure[];
}
