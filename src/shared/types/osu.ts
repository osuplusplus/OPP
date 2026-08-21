export type Ruleset = "osu" | "taiko" | "fruits" | "mania";
export type ScoreCategory = "best" | "pinned" | "recent";
export type OsuClient = "stable" | "lazer";
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

export interface AuthStatus {
  credentials_configured: boolean;
  connected: boolean;
  client_id: string | null;
  callback_url: string;
  user_id: number | null;
  username: string | null;
}

export interface PendingOAuth {
  authorization_url: string;
  expires_at: string;
}

export interface OAuthResult {
  ok: boolean;
  code: string;
  message: string;
}

export interface AppSettings {
  onboarding_version: number;
  page_onboarding_versions: Record<string, number>;
  ignored_update_version?: string | null;
  reduce_motion: boolean;
  similarity_index_directory: string | null;
  mania_similarity_index_directory: string | null;
  beatmap_download_directory: string | null;
  default_beatmap_download_provider: BeatmapDownloadProvider;
  include_video_in_beatmap_downloads: boolean;
  open_downloaded_beatmaps_after_download: boolean;
  replay_export_directory: string | null;
  danser_executable_path?: string | null;
  auto_export_new_replays_with_danser?: boolean;
  danser_render_preferences?: DanserRenderPreferences;
  tosu_executable_path: string | null;
  tosu_api_base_url: string;
  launch_tosu_with_game: boolean;
  tosu_lyrics_executable_path: string | null;
  launch_tosu_lyrics_with_tosu: boolean;
  theme_primary: ThemeColor;
  theme_secondary: ThemeColor;
  theme_mode?: ThemeMode;
  launch_tosu_on_game_detect?: boolean;
  obs_websocket_url?: string;
  obs_selected_scene?: string | null;
  launch_tosu_on_obs_detect?: boolean;
  suppress_tosu_launch_prompt?: boolean;
  game_session_analysis_on_detect?: boolean;
  preview_volume?: number;
  cache_limit_mb?: number;
  similarity_preferences: SimilarityPreferences;
}

export interface SimilarityManualWeights extends DifficultyFeatureVector {
  parameters: number;
}

export interface SimilarityPreferences {
  advanced_enabled: boolean;
  mode: "dynamic" | "manual";
  lower_sections: number;
  upper_sections: number;
  manual_weights: SimilarityManualWeights;
  results_per_page: number;
}

export interface ObsStatus {
  running: boolean;
  websocket_url: string;
  connected: boolean;
  password_configured: boolean;
  selected_scene: string | null;
  last_error: string | null;
}

export interface ObsRefreshResult {
  refreshed_sources: string[];
  skipped: boolean;
  message: string;
}

export type SimilarityIndexState =
  | "unconfigured"
  | "missing"
  | "invalid"
  | "incompatible"
  | "unsupported"
  | "ready";

export type SimilarityRuleset = Extract<Ruleset, "osu" | "mania">;
export type ManiaKeyCount = 4 | 6 | 7;

export interface SimilarityIndexStatus {
  ruleset: Ruleset;
  state: SimilarityIndexState;
  directory: string | null;
  message: string;
  record_count: number | null;
  analyzer_version: number | null;
  normalization_version: number | null;
  algorithm_id: string | null;
  data_cutoff_at: number | null;
  supports_dynamic_weighting: boolean;
  records_by_key_count: Partial<Record<ManiaKeyCount, number>> | null;
}

export interface DifficultyFeatureVector {
  aim: number;
  speed: number;
  reading: number;
  slider: number;
  overlap: number;
}

export interface SimilarityBaseFeatures {
  bpm: number;
  ar: number;
  od: number;
  cs: number;
  hp: number;
  length_seconds: number;
  object_count: number;
  object_density: number;
  circle_ratio: number;
  slider_ratio: number;
  spinner_ratio: number;
  max_combo: number;
}

export interface SimilarityBaseWeights {
  bpm: number;
  ar: number;
  length_seconds: number;
  object_density: number;
  circle_ratio: number;
  slider_ratio: number;
}

export interface SimilarityFilters {
  min_star: number | null;
  max_star: number | null;
  min_ar: number | null;
  max_ar: number | null;
  min_cs: number | null;
  max_cs: number | null;
  min_od: number | null;
  max_od: number | null;
  min_bpm: number | null;
  max_bpm: number | null;
  min_length_seconds: number | null;
  max_length_seconds: number | null;
  min_object_density: number | null;
  max_object_density: number | null;
  min_circle_ratio: number | null;
  max_circle_ratio: number | null;
  min_slider_ratio: number | null;
  max_slider_ratio: number | null;
}

export type SimilaritySource =
  | { kind: "beatmap_id"; value: string }
  | { kind: "local_file"; path: string };

export interface OsuSimilarityQueryRequest {
  ruleset: "osu";
  source: SimilaritySource;
  weighting: SimilarityWeighting;
  filters: SimilarityFilters;
  result_limit: number;
}

export interface ManiaSimilarityQueryRequest {
  ruleset: "mania";
  source: SimilaritySource;
  result_limit: number;
}

export type SimilarityQueryRequest =
  | OsuSimilarityQueryRequest
  | ManiaSimilarityQueryRequest;

export type SimilarityWeighting =
  | {
      mode: "dynamic";
      lower_sections: number;
      upper_sections: number;
    }
  | {
      mode: "manual";
      difficulty_weights: DifficultyFeatureVector;
      parameter_weight: number;
    };

export interface SimilarityParameterVector {
  ar: number;
  cs: number;
  od: number;
}

export interface SimilarityDynamicWeightProfile {
  seed_beatmap_id?: number | null;
  target_star_rating: number;
  candidate_min_section: number;
  candidate_max_section: number;
  stats_min_section: number;
  stats_max_section: number;
  sample_count: number;
  mean: DifficultyFeatureVector;
  stddev: DifficultyFeatureVector;
  delta: DifficultyFeatureVector;
  z_score: DifficultyFeatureVector;
  weights: DifficultyFeatureVector;
  parameter_mean: SimilarityParameterVector;
  parameter_stddev: SimilarityParameterVector;
  parameter_delta: SimilarityParameterVector;
  parameter_z_score: SimilarityParameterVector;
  parameter_group_z_score: number;
  parameter_weight: number;
  fallback_reason: string | null;
}

export interface SimilarityBeatmap {
  ruleset: "osu";
  beatmap_id: number;
  beatmapset_id: number;
  artist: string;
  title: string;
  version: string;
  creator: string;
  online_url: string;
  star_rating: number | null;
  difficulty: DifficultyFeatureVector;
  base: SimilarityBaseFeatures;
}

export interface SimilarityTarget extends SimilarityBeatmap {
  source: "index" | "online" | "local_file";
  analyzer_version: number;
  normalization_version: number;
}

export interface SimilarityResult extends SimilarityBeatmap {
  final_distance: number;
  difficulty_distance: number;
  base_distance: number;
}

export interface OsuSimilarityQueryResponse {
  ruleset: "osu";
  target: SimilarityTarget;
  results: SimilarityResult[];
  dynamic_profile: SimilarityDynamicWeightProfile | null;
}

export interface ManiaDifficultyVector {
  speed: number;
  hand_stream: number;
  jack: number;
  chordjack: number;
  technical: number;
  stamina: number;
  long_note: number;
  course: number;
}

export interface ManiaStyleVector {
  stream: number;
  chordstream: number;
  jacks: number;
  coordination: number;
  density: number;
  wildcard: number;
  chord_rate: number;
  large_chord_rate: number;
  rotation_rate: number;
  anchor_rate: number;
  rhythm_entropy: number;
  transition_entropy: number;
  ln_note_ratio: number;
  hold_occupancy: number;
  hybrid_row_ratio: number;
  peak_to_sustain_gap: number;
}

export interface ManiaBaseFeatures {
  bpm: number;
  length_seconds: number;
  active_length_seconds: number;
  note_count: number;
  row_count: number;
  avg_nps: number;
  peak_nps: number;
  break_density: number;
  sv_change_rate: number;
}

export type ManiaModeFamily = "rc" | "hb" | "mix" | "ln";
export type ManiaPattern =
  | "stream"
  | "chordstream"
  | "jacks"
  | "coordination"
  | "density"
  | "wildcard";

export interface ManiaSimilarityBeatmap {
  ruleset: "mania";
  beatmap_id: number;
  beatmapset_id: number;
  artist: string;
  title: string;
  version: string;
  creator: string;
  online_url: string;
  key_count: ManiaKeyCount;
  family: ManiaModeFamily;
  pattern: ManiaPattern;
  difficulty: ManiaDifficultyVector;
  style: ManiaStyleVector;
  base: ManiaBaseFeatures;
  difficulty_percentile: number;
  difficulty_band: number;
}

export interface ManiaSimilarityTarget extends ManiaSimilarityBeatmap {
  source: "index" | "online" | "local_file";
  analyzer_version: number;
  normalization_version: number;
}

export interface ManiaDistanceComponents {
  skill: number;
  pattern: number;
  structure: number;
  difficulty: number;
  context: number;
}

export interface ManiaSimilarityResult extends ManiaSimilarityBeatmap {
  final_distance: number;
  distance_components: ManiaDistanceComponents;
}

export interface ManiaSimilarityQueryResponse {
  ruleset: "mania";
  target: ManiaSimilarityTarget;
  results: ManiaSimilarityResult[];
}

export type SimilarityQueryResponse =
  | OsuSimilarityQueryResponse
  | ManiaSimilarityQueryResponse;

export type SimilarityRecommendationKind = "recent" | "best";

export interface OsuSimilarityRecommendationRequest {
  ruleset: "osu";
  kind: SimilarityRecommendationKind;
  weighting: SimilarityWeighting;
  filters: SimilarityFilters;
  result_limit: number;
  seed_limit?: number;
  excluded_beatmap_ids?: number[];
}

export interface ManiaSimilarityRecommendationRequest {
  ruleset: "mania";
  kind: SimilarityRecommendationKind;
  result_limit: number;
  seed_limit?: number;
  excluded_beatmap_ids?: number[];
}

export type SimilarityRecommendationRequest =
  | OsuSimilarityRecommendationRequest
  | ManiaSimilarityRecommendationRequest;

export interface SimilarityRecommendationResult extends SimilarityResult {
  recommended_by: SimilarityBeatmap;
}

export interface OsuSimilarityRecommendationResponse {
  ruleset: "osu";
  kind: SimilarityRecommendationKind;
  seed_count: number;
  skipped_seed_count: number;
  results: SimilarityRecommendationResult[];
  dynamic_profiles: SimilarityDynamicWeightProfile[];
}

export interface ManiaSimilarityRecommendationResult extends ManiaSimilarityResult {
  recommended_by: ManiaSimilarityBeatmap;
}

export interface ManiaSimilarityRecommendationGroup {
  key_count: ManiaKeyCount;
  seed_count: number;
  results: ManiaSimilarityRecommendationResult[];
}

export interface ManiaSimilarityRecommendationResponse {
  ruleset: "mania";
  kind: SimilarityRecommendationKind;
  seed_count: number;
  skipped_seed_count: number;
  groups: ManiaSimilarityRecommendationGroup[];
}

export type SimilarityRecommendationResponse =
  | OsuSimilarityRecommendationResponse
  | ManiaSimilarityRecommendationResponse;

export type AnySimilarityBeatmap = SimilarityBeatmap | ManiaSimilarityBeatmap;
export type AnySimilarityResult = SimilarityResult | ManiaSimilarityResult;
export type AnySimilarityRecommendationResult =
  | SimilarityRecommendationResult
  | ManiaSimilarityRecommendationResult;

export type ThemeColor = "cyan" | "blue" | "violet" | "pink" | "orange" | "green";
export type ThemeMode = "dark" | "light";

export interface TosuLyricsStatus {
  installed: boolean;
  executable_path: string | null;
  running: boolean;
  owned_by_opp: boolean;
  proxy_url: string;
}

export interface TosuStatus {
  installed: boolean;
  executable_path: string | null;
  api_base_url: string;
  api_reachable: boolean;
  running: boolean;
  owned_by_opp: boolean;
  dashboard_url: string;
  last_error: string | null;
  lyrics: TosuLyricsStatus;
}

export interface TosuLogEntry { at: string; stream: string; level: "info" | "warning" | "error"; message: string; }

export interface TosuLiveSnapshot {
  state: string | null; mode: string | null; artist: string | null; title: string | null; difficulty: string | null;
  song_time_ms: number | null; song_length_ms: number | null;
  score: number | null; combo: number | null; max_combo: number | null; accuracy: number | null;
  misses: number | null; hit_300: number | null; hit_100: number | null; hit_50: number | null;
  pp_current: number | null; pp_fc: number | null; mods: string | null;
}

export interface DefaultFileClients {
  beatmap: OsuClient;
  skin: OsuClient;
}

export interface UserSnapshot {
  captured_at: string;
  username: string;
  pp: number | null;
  ranked_score: number | null;
  hit_accuracy: number | null;
  total_hits: number | null;
  total_score: number | null;
}

export interface GameSessionSummary {
  started_at: string;
  ended_at: string | null;
  ruleset: Ruleset;
  client: string;
  executable: string;
  start: UserSnapshot;
  end: UserSnapshot | null;
  running: boolean;
}

export interface GameClientStatus {
  client: OsuClient;
  running: boolean;
  executable: string | null;
  detected_at: string;
}

export interface GameStatusSnapshot { clients: GameClientStatus[]; }

export interface ManiaConversionItem {
  input: string;
  status: "completed" | "skipped" | "failed";
  output: string | null;
  message: string | null;
}

export interface ManiaConversionResult { items: ManiaConversionItem[]; }

export interface GameMediaItem {
  client: OsuClient;
  path: string;
  kind: "replay" | "screenshot";
  modified_at: string | null;
  size: number;
}

export interface GameReplayPayload {
  path: string;
  file_name: string;
  bytes_base64: string;
  video_ready: boolean;
  note: string;
}
export interface ReplayMapInfo {
  path: string;
  beatmap_hash: string;
  username: string;
  beatmap_id: number | null;
  beatmap_resource_id: string | null;
  beatmap_title: string | null;
  submitted: boolean;
}

export type RenderSkinKind = "official" | "custom";
export type RenderDeveloperMode = "success" | "api_failure" | "websocket_failure";

export interface ReplayRenderOptions {
  resolution: "720x480" | "960x540" | "1280x720" | "1920x1080";
  global_volume: number; music_volume: number; hitsound_volume: number;
  show_hit_error_meter: boolean; show_unstable_rate: boolean; show_score: boolean; show_hp_bar: boolean;
  show_combo_counter: boolean; show_pp_counter: boolean; show_scoreboard: boolean; show_borders: boolean;
  show_mods: boolean; show_result_screen: boolean; show_hit_counter: boolean; show_key_overlay: boolean;
  show_avatars_on_scoreboard: boolean; show_aim_error_meter: boolean; show_strain_graph: boolean; show_slider_breaks: boolean;
  use_skin_cursor: boolean; use_skin_colors: boolean; use_skin_hitsounds: boolean; use_beatmap_colors: boolean;
  cursor_rainbow: boolean; cursor_trail: boolean; cursor_trail_glow: boolean; cursor_ripples: boolean; cursor_size: number;
  draw_follow_points: boolean; draw_combo_numbers: boolean; slider_snaking_in: boolean; slider_snaking_out: boolean;
  slider_merge: boolean; objects_rainbow: boolean; flash_objects: boolean; use_slider_hitcircle_color: boolean; beat_scaling: boolean;
  seizure_warning: boolean; load_storyboard: boolean; load_video: boolean;
  intro_bg_dim: number; ingame_bg_dim: number; break_bg_dim: number;
  bg_parallax: boolean; show_danser_logo: boolean; skip_intro: boolean; play_nightcore_samples: boolean; ignore_fail: boolean;
}

export interface ReplayRenderRequest {
  client: OsuClient; replay_path: string; username: string;
  options: ReplayRenderOptions; skin_kind: RenderSkinKind; skin: string;
  verification_key: string | null; developer_mode: RenderDeveloperMode | null;
}

export interface ReplayRenderJob { render_id: number; status: string; description: string; }
export interface ReplayRenderProgress { render_id: number; status: string; description: string; video_url: string | null; }

export interface DanserRenderPreferences {
  settings_profile: string;
  skin: string;
  skip: boolean;
  quickstart: boolean;
  start: number | null;
  end: number | null;
  speed: number;
  pitch: number;
  offset: number;
  mods: string;
  mods2: string;
  cs: number | null;
  ar: number | null;
  od: number | null;
  hp: number | null;
  no_db_check: boolean;
  no_update_check: boolean;
  debug: boolean;
  settings_patch: string;
  frame_width: number;
  frame_height: number;
  fps: number;
  encoder: "libx264" | "h264_nvenc" | "h264_qsv";
  quality: number;
  motion_blur: boolean;
  motion_blur_oversample: number;
}

export interface DanserStatus {
  available: boolean;
  executable_path: string | null;
  ffmpeg_available: boolean;
  profiles: string[];
  message: string;
}

export interface DanserRenderJob {
  id: string;
  replay_path: string;
  status: "queued" | "running" | "completed" | "failed" | "cancelled";
  progress: number;
  description: string;
  output_path: string | null;
  queue_position: number | null;
}

export interface DanserEnqueueRequest {
  client: OsuClient;
  replay_paths: string[];
  preferences: DanserRenderPreferences;
}

export interface NewReplayItem {
  path: string;
  file_name: string;
  beatmap_title: string | null;
  username: string | null;
  renderable: boolean;
  reason: string | null;
}

export interface NewReplaysDetected {
  client: OsuClient;
  started_at: string;
  detected_at: string;
  replays: NewReplayItem[];
}

export interface GameScreenshotPayload {
  path: string;
  file_name: string;
  mime_type: string;
  bytes_base64: string;
}

export interface DisconnectResult {
  revoked: boolean;
  warning: string | null;
}

export interface UserLevel {
  current?: number;
  progress?: number;
}

export interface GradeCounts {
  ssh?: number;
  ss?: number;
  sh?: number;
  s?: number;
  a?: number;
}

export interface UserStatistics {
  count_100?: number;
  count_300?: number;
  count_50?: number;
  count_miss?: number;
  grade_counts?: GradeCounts;
  hit_accuracy?: number;
  is_ranked?: boolean;
  level?: UserLevel;
  maximum_combo?: number;
  play_count?: number;
  play_time?: number;
  pp?: number;
  global_rank?: number | null;
  country_rank?: number | null;
  ranked_score?: number;
  replays_watched_by_others?: number;
  total_hits?: number;
  total_score?: number;
  variants?: Array<Record<string, unknown>>;
  [key: string]: unknown;
}

export interface UserCover {
  custom_url?: string | null;
  url?: string;
  id?: number | null;
}

export interface RankHistory {
  mode?: Ruleset;
  data?: number[];
}

export interface MonthlyCount {
  start_date?: string;
  count?: number;
}

export interface ProfilePage {
  html?: string;
  raw?: string;
}

export interface OwnProfile {
  id: number;
  username: string;
  avatar_url: string;
  avatar_data_url?: string | null;
  country_code: string;
  is_active: boolean;
  is_online: boolean;
  is_supporter: boolean;
  is_restricted?: boolean | null;
  last_visit?: string | null;
  playmode?: Ruleset | null;
  profile_colour?: string | null;
  default_group?: string | null;
  cover?: UserCover | null;
  cover_url?: string | null;
  country?: { code?: string; name?: string };
  statistics?: UserStatistics | null;
  statistics_rulesets?: Partial<Record<Ruleset, UserStatistics>> | null;
  rank_history?: RankHistory | null;
  monthly_playcounts?: MonthlyCount[] | null;
  replays_watched_counts?: MonthlyCount[] | null;
  badges?: Array<Record<string, any>> | null;
  groups?: Array<Record<string, any>> | null;
  user_achievements?: Array<Record<string, any>> | null;
  account_history?: Array<Record<string, any>> | null;
  page?: ProfilePage | null;
  join_date?: string;
  location?: string | null;
  interests?: string | null;
  occupation?: string | null;
  website?: string | null;
  discord?: string | null;
  twitter?: string | null;
  title?: string | null;
  previous_usernames?: string[];
  playstyle?: string[];
  post_count?: number;
  follower_count?: number;
  mapping_follower_count?: number;
  beatmap_playcounts_count?: number;
  favourite_beatmapset_count?: number;
  graveyard_beatmapset_count?: number;
  loved_beatmapset_count?: number;
  pending_beatmapset_count?: number;
  ranked_beatmapset_count?: number;
  guest_beatmapset_count?: number;
  nominated_beatmapset_count?: number;
  scores_best_count?: number;
  scores_first_count?: number;
  scores_recent_count?: number;
  kudosu?: { available?: number; total?: number };
  support_level?: number;
  rank_highest?: { rank?: number; updated_at?: string } | null;
  [key: string]: any;
}

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

export interface OnlineBeatmapSearchQuery {
  query: string;
  ruleset: Ruleset | null;
  status: string;
  genre: number | null;
  language: number | null;
  extras: Array<"video" | "storyboard">;
  include_nsfw: boolean;
  sort: string;
  artist: string;
  title: string;
  source: string;
  mapper: string;
  difficulty: string;
  tags: string;
  ranked_from: string;
  ranked_to: string;
  submitted_from: string;
  submitted_to: string;
  updated_from: string;
  updated_to: string;
  favourites_min: number | null;
  favourites_max: number | null;
  stars_min: number | null;
  stars_max: number | null;
  bpm_min: number | null;
  bpm_max: number | null;
  length_min: number | null;
  length_max: number | null;
  ar_min: number | null;
  ar_max: number | null;
  cs_min: number | null;
  cs_max: number | null;
  od_min: number | null;
  od_max: number | null;
  hp_min: number | null;
  hp_max: number | null;
  keys_min: number | null;
  keys_max: number | null;
  cursor_string: string | null;
  content_filter: string;
  grade: string;
  played: string;
}

export type BeatmapSource = "official" | "nerinyan" | "catboy";

export interface BeatmapSourceStatus {
  id: BeatmapSource;
  label: string;
  online: boolean;
  supports_search: boolean;
  supports_metadata: boolean;
  supports_osu_download: boolean;
  supports_osz_download: boolean;
  retry_after_seconds?: number | null;
  message?: string | null;
}

export interface BeatmapCalculationRequest {
  beatmap_id: number;
  mods: string[];
  accuracy?: number;
  misses?: number;
  combo?: number;
  n300?: number;
  n100?: number;
  n50?: number;
}

export interface BeatmapCalculationResult {
  beatmap_id: number;
  mods: string[];
  mode: string;
  stars: number;
  pp: number;
  max_pp: number;
  max_combo: number;
  calculation_engine: string;
  calculated_at: string;
  source: BeatmapSource;
  star_algorithm: string;
  star_algorithm_date: string;
  performance_algorithm: string;
  performance_algorithm_date: string;
}

export interface OnlineBeatmapCover {
  cover?: string;
  "cover@2x"?: string;
  card?: string;
  "card@2x"?: string;
  list?: string;
  "list@2x"?: string;
  slimcover?: string;
  "slimcover@2x"?: string;
}

export interface OnlineBeatmap {
  id: number;
  beatmapset_id: number;
  difficulty_rating: number;
  mode: Ruleset;
  mode_int?: number;
  status: string;
  total_length: number;
  hit_length?: number;
  version: string;
  accuracy?: number;
  ar?: number;
  bpm?: number;
  convert?: boolean;
  count_circles?: number;
  count_sliders?: number;
  count_spinners?: number;
  cs?: number;
  drain?: number;
  passcount?: number;
  playcount?: number;
  last_updated?: string;
  url?: string;
  checksum?: string;
}

export type CollectionSource = "opp" | "stable" | "lazer";

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
  entries: CollectionEntry[];
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
}

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

export interface BeatmapHubAuthStatus {
  has_identity: boolean;
  connected: boolean;
  public_key: string | null;
  user_id: string | null;
  device_id: string | null;
  display_name: string | null;
  device_name: string | null;
  expires_at: string | null;
}

export interface BeatmapHubDevice {
  id: string;
  device_name: string;
  public_key: string;
  created_at: string;
  last_seen_at: string;
  revoked_at: string | null;
}

export interface BeatmapHubProfile {
  user: { id: string; display_name: string };
  current_device_id: string;
  devices: BeatmapHubDevice[];
}

export interface BeatmapHubPack {
  id: string;
  title: string;
  description: string;
  is_private: boolean;
  owner: { id: string; display_name: string };
  beatmapset_ids: number[];
  stars_min?: number | null;
  stars_max?: number | null;
  manifest_hash: string;
  rating: { average: number | null; count: number };
  likes: { count: number };
  comments: { count: number };
  viewer: { rating: number | null; favorited: boolean; liked: boolean; can_edit: boolean } | null;
  created_at: string;
  updated_at: string;
}

export interface OsekaiMedal {
  Medal_ID: number;
  Name?: string;
  Description?: string;
  Instructions?: string;
  Solution?: string;
  Link?: string;
  Date_Released?: string;
  [key: string]: unknown;
}

export interface OsekaiMedalBeatmap {
  Beatmap_ID?: number;
  Beatmapset_ID?: number;
  Title?: string;
  Artist?: string;
  Version?: string;
  Creator?: string;
  Note?: string;
  Song_Title?: string;
  Song_Artist?: string;
  Difficulty_Name?: string;
  [key: string]: unknown;
}

export interface BeatmapHubComment {
  id: string;
  user: { id: string; display_name: string };
  content: string;
  created_at: string;
  updated_at: string;
}

export type BeatmapHubRecommendation = BeatmapHubPack;

export interface BeatmapHubPackPreview {
  pack: BeatmapHubPack;
  locally_available_ids: number[];
  missing_ids: number[];
}

export interface BeatmapHubPublishResult {
  id: string;
  included: number;
  skipped: number;
}

export interface BeatmapHubImportResult {
  folder_id: string;
  imported_sets: number;
  imported_entries: number;
  unresolved_sets: number;
}

export interface BeatmapHubDeviceLink {
  link_token: string;
  expires_at: string;
}

export interface OnlineBeatmapset {
  id: number;
  user_id?: number;
  artist: string;
  artist_unicode?: string;
  title: string;
  title_unicode?: string;
  creator: string;
  source?: string;
  status: string;
  ranked_date?: string | null;
  submitted_date?: string | null;
  last_updated?: string | null;
  bpm?: number;
  favourite_count?: number;
  play_count?: number;
  preview_url?: string;
  nsfw?: boolean;
  video?: boolean;
  storyboard?: boolean;
  tags?: string;
  covers?: OnlineBeatmapCover;
  beatmaps?: OnlineBeatmap[];
  genre?: { id?: number; name?: string };
  language?: { id?: number; name?: string };
  ratings?: number[];
  availability?: {
    download_disabled?: boolean;
    more_information?: string | null;
  };
  description?: { description?: string | null };
}

export interface OnlineBeatmapSearchResponse {
  beatmapsets: OnlineBeatmapset[];
  cursor_string?: string | null;
  total?: number;
}

export interface CollectedBeatmapsets {
  items: OnlineBeatmapset[];
  available_total: number | null;
  truncated: boolean;
}

export interface BeatmapDownloadItem {
  beatmapset_id: number;
  artist: string;
  title: string;
}

export interface BeatmapDownloadRequest {
  destination: string;
  items: BeatmapDownloadItem[];
  provider: BeatmapDownloadProvider | "none";
  overwrite: boolean;
  include_video: boolean;
  open_after_download?: boolean;
}

export interface BeatmapDownloadFailure {
  beatmapset_id: number;
  title: string;
  message: string;
}

export interface BeatmapDownloadResult {
  destination: string;
  total: number;
  completed: number;
  skipped: number;
  failed: number;
  cancelled: boolean;
  failures: BeatmapDownloadFailure[];
  completed_paths?: string[];
}

export interface BeatmapDownloadProgress {
  phase:
    | "started"
    | "downloading"
    | "completed"
    | "skipped"
    | "failed"
    | "finished"
    | "cancelled";
  total: number;
  processed: number;
  completed: number;
  skipped: number;
  failed: number;
  current_beatmapset_id: number | null;
  current_title: string | null;
  message: string | null;
  downloaded_bytes?: number;
  total_bytes?: number | null;
  bytes_per_second?: number;
  completed_paths?: string[];
  destination?: string;
}

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

export interface SkinConfigEntry {
  key: string;
  value: string;
  color?: number[] | null;
}

export interface SkinConfigSection {
  name: string;
  entries: SkinConfigEntry[];
}

export interface SkinInventory {
  file_count: number;
  total_bytes: number;
  by_extension: Record<string, number>;
}

export interface LocalSkinSummary {
  resource: LocalResourceRef;
  completeness: Completeness;
  name: string;
  author: string;
  version: string;
  section_count: number;
  has_mania_config: boolean;
  resource_count: number | null;
  total_bytes: number | null;
  modified_at: string | null;
  accent_colors: number[][];
}

export interface LocalSkinDetail {
  summary: LocalSkinSummary;
  sections: SkinConfigSection[];
  inventory: SkinInventory | null;
  notice: string | null;
}

export type SkinAssetKind = "image" | "audio";

export interface LocalSkinAssetSummary {
  resource_id: string;
  kind: SkinAssetKind;
  name: string;
  logical_path: string;
  extension: string;
  size: number;
  category: string;
}

export interface LocalSkinPreview {
  skin_resource_id: string;
  completeness: Completeness;
  images: LocalSkinAssetSummary[];
  sounds: LocalSkinAssetSummary[];
}

export interface LocalSkinAssetPayload {
  resource_id: string;
  kind: SkinAssetKind;
  mime_type: string;
  data_url: string;
}

export type WorkshopAssetKind = "image" | "audio";

export interface SkinAssetVariant {
  asset_id: string;
  kind: WorkshopAssetKind;
  name: string;
  logical_path: string;
  extension: string;
  size: number;
  scale: number;
  frame: number | null;
}

export interface SkinTreeNode {
  part_id: string;
  part_key: string;
  label: string;
  path_segments: string[];
  asset_count: number;
  image_count: number;
  audio_count: number;
  children: SkinTreeNode[];
}

export interface SkinTree {
  skin_resource_id: string;
  roots: SkinTreeNode[];
}

export interface SkinPartPreview {
  skin_resource_id: string;
  part_key: string;
  assets: SkinAssetVariant[];
}

export interface SkinWorkshopAssetPayload {
  asset_id: string;
  kind: WorkshopAssetKind;
  mime_type: string;
  data_url: string;
}

export interface SkinWorkshopConfigEntry {
  key: string;
  value: string;
  occurrence: number;
  line: number;
}

export interface SkinWorkshopConfigSection {
  name: string;
  entries: SkinWorkshopConfigEntry[];
}

export interface SkinConfigDocument {
  source: string;
  sections: SkinWorkshopConfigSection[];
  errors: Array<{ line: number; message: string }>;
  encoding: string;
  newline: string;
}

export type SkinWorkshopAction =
  | { type: "replace_component"; target_logical_path: string; replacement_path: string }
  | { type: "replace_part"; target_part_key: string; source_skin_resource_id: string }
  | { type: "copy_component"; target_logical_path: string; source_skin_resource_id: string; source_logical_path: string }
  | { type: "copy_config_entry"; source_skin_resource_id: string; section: string; key: string; occurrence: number }
  | { type: "update_config_source"; source: string }
  | { type: "update_config_entry"; section: string; key: string; occurrence: number; value: string };

export type SkinWorkshopWriteMode =
  | { mode: "overwrite" }
  | { mode: "create_copy"; name: string };

export type SkinWorkshopPreset =
  | { type: "migrate_mania"; source_skin_resource_id: string };

export interface SkinWorkshopMutationResult {
  name: string;
  path: string;
  created_copy: boolean;
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
