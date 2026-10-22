// 从 types/osu.ts 按域拆分；types/osu.ts 是对外的 barrel，消费方导入路径不变。
import type { OsuClient, Ruleset } from "./common";
import type { LocalBeatmapSummary } from "./localAnalysis";
import type { OnlineBeatmap } from "./onlineBeatmaps";

export type CollectionSource = "opp" | "stable" | "lazer";

export interface LocalScoreRecord {
  id: string; source: "stable" | "lazer" | "manual"; player: string; ruleset: string;
  scoring: string; beatmap_hash: string; score: number; accuracy: number | null;
  combo: number | null; mods: string; played_at: string | null; note: string;
}
export interface CollectionPersonalRecord {
  revision: number; tags: { name: string; color: string }[]; note: string;
  slot_override: string | null; scores: LocalScoreRecord[]; representative: LocalScoreRecord | null;
}
export interface CollectionPoolSnapshot extends TournamentInfo {
  title?: string;
  reference: TournamentPoolRef;
  slots: { beatmap_id: number; label: string; selected_by: string; comment: string; is_custom?: boolean; is_original?: boolean; download_disabled?: boolean; metadata?: OnlineBeatmap | null }[];
}
export interface CollectionBrowseQuery {
  folder_id: string | null; search: string; sort: string; offset: number; limit: number; player: string | null;
}
export interface CollectionBrowseRow {
  key: string; folder_id: string; folder_name: string; read_only: boolean;
  entry: CollectionEntry; local: LocalBeatmapSummary | null; metadata?: OnlineBeatmap | null;
  slot: string; source_slot: string; selected_by: string; pool_comment: string;
  is_custom?: boolean; is_original?: boolean;
  record: CollectionPersonalRecord; latest_score: LocalScoreRecord | null;
  representative_available: boolean; matches: { field: string; text: string }[];
}
export interface CollectionBrowsePage {
  items: CollectionBrowseRow[]; total: number; offset: number; limit: number;
  matching_folders: string[]; pool: CollectionPoolSnapshot | null;
  overview?: CollectionOverview;
}
export interface CollectionOverview {
  local_count: number; marked_count: number; noted_count: number;
  stars: [number, number] | null; bpm: [number, number] | null;
  total_length_ms: number; slots: [string, number][];
}
export interface LocalArtwork { client: OsuClient; resource_id: string }
export type CollectionArtwork = LocalArtwork;
export interface LocalScoreStatus { players: string[]; default_player: string | null; errors: string[]; count: number }

export interface RinoPoolRef {
  provider: "rino";
  season: "s1" | "s2";
  category: "qualification" | "ro16" | "quarterfinals" | "semifinals" | "finals" | "grandfinals";
}

export type TournamentPoolRef = RinoPoolRef | { provider: "opp"; url: string };
export interface TournamentInfo { tournament?: string; season?: string; category?: string }
export type TournamentImportPhase = "fetching" | "enriching" | "saving";
export interface TournamentImportProgress { request_id: number; phase: TournamentImportPhase }

export interface TournamentPoolEntry {
  beatmap_id: number;
  selection_type: string;
  position: number;
  selected_by: string | null;
  selected_by_name: string | null;
  comment: string;
  is_custom: boolean;
  is_original: boolean;
  beatmap: {
    beatmapset_id: number;
    title: string;
    artist: string;
    creator: string;
    difficulty_name: string;
    checksum: string | null;
    download_disabled: boolean;
  } | null;
  resolution_error: string | null;
}

export interface TournamentPool extends TournamentInfo {
  reference: TournamentPoolRef;
  title: string;
  entries: TournamentPoolEntry[];
}

export interface TournamentLink {
  id: number;
  reference: TournamentPoolRef;
}

export interface TournamentPoolSyncResult {
  folder_id: string;
  entry_count: number;
  pool: TournamentPool;
}

export interface CollectionEntry {
  id: string;
  beatmap_id: number | null;
  beatmapset_id: number | null;
  checksum: string | null;
  ruleset: Ruleset | null;
  difficulty_name: string;
  title: string;
  artist: string;
  creator: string;
  resolved: boolean;
}

export interface CollectionFolder {
  id: string;
  name: string;
  creator: string;
  created_at: string;
  updated_at: string;
  source: CollectionSource;
  read_only: boolean;
  pending_write: boolean;
  stable_sync?: boolean;
  pool?: CollectionPoolSnapshot | null;
  entries: CollectionEntry[];
  external_id?: string | null;
  external_fingerprint?: string | null;
  last_read_at?: string | null;
  backup_path?: string | null;
  backup_fingerprint?: string | null;
  backup_confirmed_at?: string | null;
}

export interface CollectionCandidate {
  beatmap_id: number | null;
  beatmapset_id: number | null;
  checksum: string | null;
  ruleset: Ruleset | null;
  difficulty_name: string;
  title: string;
  artist: string;
  creator: string;
  local_client?: OsuClient | null;
  local_resource_id?: string | null;
}

export interface CollectionSourceStatus {
  client: OsuClient;
  available: boolean;
  read_only: boolean;
  message: string;
  refreshed_at: string | null;
}

export interface CollectionSnapshot {
  folders: CollectionFolder[];
  sources: CollectionSourceStatus[];
}

export interface CollectionSyncStatus {
  available: boolean;
  in_sync: boolean;
  pending_changes: boolean;
  game_changed: boolean;
  missing_downloadable_count: number;
  missing_unresolved_count: number;
  sources?: CollectionSourceSyncStatus[];
}
export interface CollectionSourceSyncStatus { client: OsuClient; available: boolean; external_changed: boolean; pending_write: boolean; backup_available: boolean; backup_confirmed: boolean; backup_count: number; latest_backup: string | null; }
export interface CollectionManagerStatus { configured: boolean; available: boolean; protocol_version: string | null; version: string | null; operations: string[]; message: string; }
export interface CollectionBackupStatus { client: OsuClient; target: string | null; fingerprint: string | null; backups: string[]; latest: string | null; }

export interface CollectionSharePreview {
  name: string;
  creator: string;
  created_at: string;
  exported_at: string;
  entries: CollectionEntry[];
  available_count: number;
  downloadable_count: number;
  unresolved_count: number;
}

export interface CollectionDownloadItem {
  beatmapset_id: number;
  artist: string;
  title: string;
}

export interface CollectionInstallResult {
  installed_sets: number;
  resolved_entries: number;
  unresolved_entries: number;
}

export interface CollectionOpenResult {
  opened: number;
  failed: number;
  failures: string[];
}

export interface BeatmapOpenResult {
  opened: number;
  failed: number;
  failures: string[];
}

export interface CollectionTaskProgress {
  phase: "checking" | "installing" | "opening";
  processed: number;
  total: number;
  message: string;
}

export interface CollectionWriteResult {
  written_folders: number;
  skipped_entries: number;
  backup_path: string | null;
}

export interface CollectionFolderSummary {
  id: string; name: string; creator: string; source: CollectionSource;
  read_only: boolean; pending_write: boolean; stable_sync?: boolean; entry_count: number; missing_count: number; beatmapset_count: number; revision: number;
}
export interface CollectionSummaries { folders: CollectionFolderSummary[]; sources: CollectionSourceStatus[] }
export interface CollectionEntryPage { items: CollectionEntry[]; total: number; offset: number; limit: number; revision: number }
