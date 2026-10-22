// 从 types/osu.ts 按域拆分；types/osu.ts 是对外的 barrel，消费方导入路径不变。
import type { OsuClient, Ruleset } from "./common";
import type { CapabilityLevel, Completeness } from "./localResources";
import type { OwnProfile } from "./profile";

export interface LocalCapabilities {
  beatmaps: CapabilityLevel;
  difficulty: CapabilityLevel;
  skins: CapabilityLevel;
  skin_resources: CapabilityLevel;
  realm_index: boolean;
}

export interface LocalSourceStatus {
  client: OsuClient;
  mode: "auto" | "override";
  configured_path: string | null;
  install_root: string | null;
  data_root: string | null;
  version: string | null;
  valid: boolean;
  validation_errors: string[];
  capabilities: LocalCapabilities;
  last_scanned_at: string | null;
}

export interface LocalIndexLoadStatus {
  phase: "loading" | "ready" | "error";
  error: string | null;
  clients?: Partial<Record<OsuClient, LocalIndexClientStatus>>;
}

export interface SkillVector {
  stamina: number;
  tenacity: number;
  agility: number;
  accuracy: number;
  precision: number;
  reaction: number;
  memory: number;
  reading: number;
}

export interface SkillContribution {
  beatmap_id: number;
  title: string;
  artist: string;
  version: string;
  creator: string;
  mods: string[];
  pp: number | null;
  accuracy: number;
  combo: number | null;
  max_combo: number | null;
  misses: number;
  weight: number;
  source: "local" | "online";
  resource_id: string | null;
  skills: SkillVector;
  weighted_skills: SkillVector;
  error: string | null;
}

export interface SkillCoverage {
  requested_scores: number;
  analyzed_scores: number;
  reused_scores: number;
  local_scores: number;
  online_scores: number;
  skipped_scores: number;
  skipped_reasons: string[];
}

export interface SkillAnalysisRequest {
  ruleset: "osu";
  client: OsuClient;
  score_limit: number;
  include_online: boolean;
  force_refresh: boolean;
}

export interface SkillAnalysisResult {
  player: Pick<OwnProfile, "id" | "username" | "country_code" | "avatar_url" | "avatar_data_url">;
  ruleset: "osu";
  algorithm: { id: string; version: string; source: string };
  skills: SkillVector;
  contributions: SkillContribution[];
  coverage: SkillCoverage;
  fetched_at: string;
  stale: boolean;
}

export interface LocalIndexClientStatus {
  phase: "idle" | "watching" | "pending" | "scanning" | "error" | string;
  pending_changes: number;
  last_change_at: string | null;
  last_scan_at: string | null;
  added: number;
  modified: number;
  removed: number;
  reused: number;
}

export interface LocalResourceRef {
  resource_id: string;
  client: OsuClient;
  content_hash: string;
  logical_path?: string | null;
}

export interface ScanDiagnostic {
  code: string;
  message: string;
  logical_path?: string | null;
  resource_id?: string | null;
}

export interface LocalLibrarySummary {
  client: OsuClient;
  completeness: Completeness;
  source_root: string;
  scanned_at: string;
  beatmap_count: number;
  beatmap_set_count: number;
  beatmap_set_count_inferred: boolean;
  skin_count: number;
  source_file_count: number;
  source_bytes: number;
  diagnostic_count: number;
  mode_counts: Partial<Record<Ruleset, number>>;
  calculation: LocalCalculationVersion;
}

export interface LocalCalculationVersion {
  engine: string;
  engine_version: string;
  engine_released_at: string;
  upstream_repository: string;
  upstream_revision: string;
  upstream_date: string;
  ruleset_versions: Partial<Record<Ruleset, number>>;
  modifiers: string;
  performance_assumption: string;
}

export interface HitObjectCounts {
  circles: number;
  sliders: number;
  spinners: number;
  holds: number;
  total: number;
}

export interface LocalBeatmapSummary {
  resource: LocalResourceRef;
  set_key: string;
  set_grouping_inferred: boolean;
  beatmap_id: number | null;
  beatmap_set_id: number | null;
  title: string;
  title_unicode: string;
  artist: string;
  artist_unicode: string;
  creator: string;
  difficulty_name: string;
  ruleset: Ruleset;
  format_version: number;
  stars: number | null;
  max_pp: number | null;
  max_combo: number | null;
  bpm: number;
  length_ms: number;
  object_count: number;
  cs: number;
  ar: number;
  od: number;
  hp: number;
  average_nps: number;
  peak_nps: number;
  modified_at: string | null;
  analysis_status: string;
}

export interface StrainSeries {
  key: string;
  values: number[];
}

export interface StrainAnalysis {
  first_object_time_ms: number;
  section_start_time_ms?: number;
  section_length_ms: number;
  series: StrainSeries[];
}

export interface BeatmapPreviewInspection {
  bid: number;
  title: string;
  title_unicode: string;
  artist: string;
  artist_unicode: string;
  creator: string;
  difficulty_name: string;
  ruleset: Ruleset;
  length_ms: number;
  strains?: StrainAnalysis | null;
}

export interface BeatmapPreviewRequest {
  bid: number;
  start_seconds: number | null;
  end_seconds: number | null;
  mods?: string[];
}

export interface BeatmapPreviewResult {
  output_path: string;
  file_name: string;
  mime_type: "image/gif" | "image/png";
}

export interface LocalBeatmapDetail {
  summary: LocalBeatmapSummary;
  source: string;
  tags: string;
  background_file: string;
  audio_file: string;
  cs: number;
  ar: number;
  od: number;
  hp: number;
  slider_multiplier: number;
  slider_tick_rate: number;
  hit_objects: HitObjectCounts;
  break_count: number;
  break_duration_ms: number;
  timing_point_count: number;
  active_length_ms: number;
  average_nps: number;
  peak_nps: number;
  difficulty_algorithm: string;
  calculation: LocalCalculationVersion;
  calculated_at: string;
  strains?: StrainAnalysis | null;
}

export interface Page<T> {
  items: T[];
  total: number;
  offset: number;
  limit: number;
}

export type BeatmapSort =
  | "title"
  | "artist"
  | "creator"
  | "stars"
  | "bpm"
  | "length"
  | "object_count"
  | "modified_at";

export interface BeatmapQuery {
  client: OsuClient;
  search: string;
  rulesets: Ruleset[];
  min_stars: number | null;
  max_stars: number | null;
  min_bpm: number | null;
  max_bpm: number | null;
  min_length_ms: number | null;
  max_length_ms: number | null;
  min_objects: number | null;
  max_objects: number | null;
  min_ar: number | null;
  max_ar: number | null;
  min_cs: number | null;
  max_cs: number | null;
  min_od: number | null;
  max_od: number | null;
  submitted: boolean | null;
  sort: BeatmapSort;
  direction: "asc" | "desc";
  offset: number;
  limit: number;
}

export type SkinSort = "name" | "author" | "size" | "modified_at";

export interface SkinQuery {
  client: OsuClient;
  search: string;
  sort: SkinSort;
  direction: "asc" | "desc";
  offset: number;
  limit: number;
}

export interface LocalScanProgress {
  client: OsuClient;
  phase:
    | "discovery"
    | "indexing"
    | "beatmaps"
    | "difficulty"
    | "skins"
    | "finalizing";
  processed: number;
  total: number;
  percent: number;
}
