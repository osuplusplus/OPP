import { useQuery } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import type {
  AppSettings,
  FfmpegStatusInfo,
  AuthStatus,
  BeatmapHubAuthStatus,
  BeatmapHubComment,
  BeatmapHubDeviceLink,
  BeatmapHubImportResult,
  BeatmapHubPack,
  BeatmapHubRecommendation,
  BeatmapHubPackPreview,
  BeatmapHubProfile,
  BeatmapHubPublishResult,
  BeatmapDownloadProgress,
  BeatmapDownloadRequest,
  BeatmapDownloadResult,
  BeatmapPreviewInspection,
  BeatmapPreviewRequest,
  BeatmapPreviewResult,
  CollectionCandidate,
  CollectionDownloadItem,
  CollectionFolder,
  CollectionInstallResult,
  CollectionOpenResult,
  CollectionSharePreview,
  CollectionSnapshot,
  CollectionSyncStatus,
  CollectionTaskProgress,
  CollectionWriteResult,
  CollectionManagerStatus, CollectionBackupStatus,
  BeatmapCalculationRequest,
  BeatmapCalculationResult,
  BeatmapSourceStatus,
  Cached,
  CollectedBeatmapsets,
  CommandError,
  DisconnectResult,
  DanserEnqueueRequest,
  DanserRenderJob,
  DanserStatus,
  BeatmapQuery,
  LocalBeatmapDetail,
  LocalBeatmapSetSummary,
  LocalBeatmapSummary,
  LocalLibrarySummary,
  LocalIndexLoadStatus,
  LocalScanProgress,
  LocalSkinAssetPayload,
  LocalSkinDetail,
  LocalSkinPreview,
  LocalSkinSummary,
  GameMediaItem,
  GameReplayPayload,
  ReplayMapInfo,
  GameSessionSummary,
  GameStatusSnapshot,
  GameScreenshotPayload,
  DefaultFileClients,
  ManiaConversionResult,
  LocalSourceStatus,
  OAuthResult,
  OnlineBeatmapSearchQuery,
  OnlineBeatmapSearchResponse,
  OnlineBeatmapset,
  OsuClient,
  OwnProfile,
  Page,
  PendingOAuth,
  Ruleset,
  ScoreCategory,
  ReplayRenderJob,
  ReplayRenderProgress,
  ReplayRenderRequest,
  Score,
  SkinQuery,
  SkinConfigDocument,
  SkinPartPreview,
  SkinTree,
  SkinWorkshopAction,
  SkinWorkshopAssetPayload,
  SkinWorkshopMutationResult,
  SkinWorkshopPreset,
  SkinWorkshopWriteMode,
  SimilarityIndexStatus,
  SimilarityQueryRequest,
  SimilarityQueryResponse,
  SimilarityRecommendationRequest,
  SimilarityRecommendationResponse,
  TosuLiveSnapshot,
  TosuLogEntry,
  TosuStatus,
  TrainerRequest,
  TrainerResult,
  ObsRefreshResult,
  LazerDiskUsage,
  LazerDedupeProgress,
  LazerDedupeResult,
  ObsStatus,
  PlatformCapabilities,
  NewReplaysDetected,
} from "../types/osu";

export const isTauri = () => "__TAURI_INTERNALS__" in window;

export interface UpdateCheckResult {
  current_version: string;
  latest_version: string;
  latest_tag: string;
  is_latest: boolean;
  release_name: string | null;
  release_url: string;
  published_at: string | null;
  release_notes: string | null;
  can_auto_update: boolean;
  download_size: number | null;
}

export interface UpdateProgress {
  phase: "downloading" | "preparing" | "restarting";
  downloaded_bytes: number;
  total_bytes: number;
  message: string;
}

function normalizeError(error: unknown): CommandError {
  if (typeof error === "object" && error && "code" in error && "message" in error) {
    return error as CommandError;
  }
  if (typeof error === "string") {
    try {
      const parsed = JSON.parse(error);
      if (parsed?.code && parsed?.message) return parsed;
    } catch {
      return { code: "UNKNOWN_ERROR", message: error };
    }
    return { code: "UNKNOWN_ERROR", message: error };
  }
  return {
    code: "UNKNOWN_ERROR",
    message: error instanceof Error ? error.message : "发生未知错误",
  };
}

async function call<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!isTauri()) {
    const preview = browserPreviewValue<T>(command, args);
    if (preview !== undefined) return preview;
    throw {
      code: "TAURI_REQUIRED",
      message: "请通过 OPP 桌面应用运行此功能",
    } satisfies CommandError;
  }
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw normalizeError(error);
  }
}

function browserPreviewValue<T>(command: string, args?: Record<string, unknown>): T | undefined {
  if (command === "get_capabilities") return { os: "windows", display_gamma: true, file_association: true } as T;
  if (command === "get_beatmaphub_auth_status") return { has_identity: false, connected: false, public_key: null, user_id: null, device_id: null, display_name: null, device_name: "Preview PC", expires_at: null } as T;
  if (command === "get_beatmaphub_recommendations") return [] as T;
  if (command === "list_collections") return { folders: [], sources: [] } as T;
  if (command === "get_lazer_disk_usage") return { path: "C:\\osu!", total_size: 1610612736, unique_size: 536870912, file_count: 4096 } as T;
  if (command === "export_local_beatmap_set") return `${args?.outDir ?? "C:\\Export"}/export.osz` as T;
  if (command === "export_local_skin") return `${args?.outDir ?? "C:\\Export"}/export.osk` as T;
  if (command === "dedupe_lazer_files") return { dry_run: args?.dryRun !== false, cancelled: false, lazer_files_root: "C:\\osu\\files", stable_roots: ["C:\\osu!\\Songs"], lazer_file_count: 4096, lazer_total_size: 1610612736, already_linked_count: 1024, already_linked_size: 402653184, hashed_stable_count: 2048, candidate_count: 819, reclaimable_size: 645922816, linked_count: 0, linked_size: 0, skipped_cross_volume_count: 0, skipped_cross_volume_size: 0, failed_count: 0, failed: [] } as T;
  const profile = {
    id: 10001, username: "Preview User", avatar_url: "https://a.ppy.sh/10001", country_code: "CN",
    is_active: true, is_online: true, is_supporter: true, playmode: "osu", statistics: {
      pp: 8421, global_rank: 1234, country_rank: 88, hit_accuracy: 98.42, play_count: 1280,
      play_time: 43200, total_score: 1200000000, ranked_score: 900000000, total_hits: 800000,
      maximum_combo: 2200, level: { current: 99, progress: 72 }, grade_counts: { ssh: 2, ss: 12, sh: 30, s: 420, a: 600 },
    }, statistics_rulesets: null,
  };
  if (command === "get_own_profile") return { data: profile, fetched_at: new Date().toISOString(), stale: false } as T;
  if (command === "get_osekai_medals") return { content: [{ Medal_ID: 1, Name: "Preview Medal", Description: "完成一次练习", Instructions: "在 osu! 中完成目标。", Link: "all-secret-jackpot.png" }] } as T;
  if (command === "get_osekai_medal_detail") return { content: [{ Medal_ID: Number(args?.medalId ?? 1), Name: "Preview Medal", Description: "完成一次练习", Instructions: "在 osu! 中完成目标。", Solution: "完成目标即可解锁。", Link: "all-secret-jackpot.png" }] } as T;
  if (command === "get_osekai_medal_beatmaps") return { content: [] } as T;
  if (command === "get_scores") return { data: [], fetched_at: new Date().toISOString(), stale: false } as T;
  if (command === "get_game_status") return { clients: [{ client: "stable", running: false, executable: null, detected_at: new Date().toISOString() }, { client: "lazer", running: false, executable: null, detected_at: new Date().toISOString() }] } as T;
  if (command === "get_game_session_status") return null as T;
  if (command === "get_local_sources") return [] as T;
  if (command === "list_game_media") return [] as T;
  if (command === "get_tosu_status") return { installed: false, executable_path: null, api_base_url: "http://127.0.0.1:24050", api_reachable: false, running: false, owned_by_opp: false, dashboard_url: "http://127.0.0.1:24050", last_error: null, lyrics: { installed: false, executable_path: null, running: false, owned_by_opp: false, proxy_url: "http://127.0.0.1:41280/lyrics/" } } as T;
  if (command === "get_obs_status") return { running: false, websocket_url: "ws://127.0.0.1:4455", connected: false, password_configured: false, selected_scene: null, last_error: null } as T;
  if (command === "get_obs_scenes") return ["直播场景", "练习场景"] as T;
  if (command === "refresh_selected_obs_scene") return { refreshed_sources: [], skipped: true, message: "预览模式未连接 OBS" } as T;
  if (command === "get_default_file_clients") return { beatmap: "stable", skin: "stable" } as T;
  if (command === "check_for_updates") return {
    current_version: __APP_VERSION__,
    latest_version: __APP_VERSION__,
    latest_tag: `v${__APP_VERSION__}`,
    is_latest: true,
    release_name: `OPP v${__APP_VERSION__}`,
    release_url: "https://github.com/osuplusplus/OPP/releases/latest",
    published_at: null,
    release_notes: "当前为浏览器预览版本。",
    can_auto_update: false,
    download_size: null,
  } as T;
  if (command === "get_similarity_index_status") return {
    ruleset: args?.ruleset === "mania" || args?.ruleset === "taiko" || args?.ruleset === "fruits" ? args.ruleset : "osu",
    state: args?.ruleset === "taiko" || args?.ruleset === "fruits" ? "unsupported" : "unconfigured",
    directory: null,
    message: args?.ruleset === "taiko" || args?.ruleset === "fruits" ? "当前模式暂不支持相似谱面。" : "尚未配置本地相似谱面索引。",
    record_count: null,
    analyzer_version: null,
    normalization_version: null,
    algorithm_id: null,
    data_cutoff_at: null,
    supports_dynamic_weighting: false,
    records_by_key_count: null,
  } as T;
  if (command === "configure_similarity_index") return {
    ruleset: args?.ruleset === "mania" || args?.ruleset === "taiko" || args?.ruleset === "fruits" ? args.ruleset : "osu",
    state: args?.ruleset === "taiko" || args?.ruleset === "fruits" ? "unsupported" : args?.directory ? "ready" : "unconfigured",
    directory: args?.ruleset === "taiko" || args?.ruleset === "fruits" ? null : args?.directory ?? null,
    message: args?.ruleset === "taiko" || args?.ruleset === "fruits" ? "当前模式暂不支持相似谱面。" : args?.directory ? "本地索引已就绪。" : "尚未配置本地相似谱面索引。",
    record_count: null,
    analyzer_version: null,
    normalization_version: null,
    algorithm_id: null,
    data_cutoff_at: null,
    supports_dynamic_weighting: (args?.ruleset == null || args.ruleset === "osu") && Boolean(args?.directory),
    records_by_key_count: args?.ruleset === "mania" && args?.directory ? { 4: 18550, 6: 800, 7: 4201 } : null,
  } as T;
  if (command === "query_similar_beatmaps" || command === "recommend_similar_beatmaps") {
    const similarityRequest = args?.request as SimilarityQueryRequest | SimilarityRecommendationRequest | undefined;
    if (similarityRequest?.ruleset === "mania") {
      const difficulty = { speed: 0.72, hand_stream: 0.68, jack: 0.44, chordjack: 0.61, technical: 0.57, stamina: 0.64, long_note: 0.18, course: 0.51 };
      const style = { stream: 0.72, chordstream: 0.58, jacks: 0.37, coordination: 0.49, density: 0.66, wildcard: 0.21, chord_rate: 0.34, large_chord_rate: 0.12, rotation_rate: 0.48, anchor_rate: 0.22, rhythm_entropy: 0.59, transition_entropy: 0.54, ln_note_ratio: 0.08, hold_occupancy: 0.06, hybrid_row_ratio: 0.04, peak_to_sustain_gap: 0.31 };
      const base = { bpm: 180, length_seconds: 132, active_length_seconds: 116, note_count: 812, row_count: 687, avg_nps: 7, peak_nps: 12.4, break_density: 0.12, sv_change_rate: 0 };
      const target = { ruleset: "mania" as const, beatmap_id: 3001, beatmapset_id: 701, artist: "Synthetic Artist", title: "Key Reference", version: "4K Another", creator: "Preview Mapper", online_url: "https://osu.ppy.sh/beatmaps/3001", key_count: 4 as const, family: "rc" as const, pattern: "stream" as const, difficulty, style, base, difficulty_percentile: 0.78, difficulty_band: 7 };
      const results = [
        { ...target, beatmap_id: 3101, beatmapset_id: 711, artist: "Parallel Keys", title: "Stream Motion", version: "4K Hyper", difficulty: { ...difficulty, speed: 0.7 }, final_distance: 0.054, distance_components: { skill: 0.04, pattern: 0.06, structure: 0.08, difficulty: 0.03, context: 0.05 } },
        { ...target, beatmap_id: 3102, beatmapset_id: 712, artist: "Night Matrix", title: "Hand Balance", version: "4K Another", family: "hb" as const, pattern: "coordination" as const, final_distance: 0.089, distance_components: { skill: 0.07, pattern: 0.09, structure: 0.11, difficulty: 0.05, context: 0.08 } },
      ];
      if (command === "recommend_similar_beatmaps") {
        return { ruleset: "mania", kind: "kind" in similarityRequest ? similarityRequest.kind : "recent", seed_count: 12, skipped_seed_count: 1, groups: [{ key_count: 4, seed_count: 12, results: results.map((result) => ({ ...result, recommended_by: target })) }] } as T;
      }
      return { ruleset: "mania", target: { ...target, source: "index", analyzer_version: 1, normalization_version: 1 }, results } as T;
    }
    const feature = { aim: 0.72, speed: 0.64, reading: 0.81, slider: 0.28, overlap: 0.57 };
    const base = { bpm: 186, ar: 9.2, od: 8.6, cs: 4, hp: 6, length_seconds: 124, object_count: 612, object_density: 4.94, circle_ratio: 0.58, slider_ratio: 0.4, spinner_ratio: 0.02, max_combo: 902 };
    const target = { ruleset: "osu" as const, beatmap_id: 1001, beatmapset_id: 501, artist: "Synthetic Artist", title: "Reference Pattern", version: "Insane", creator: "Preview Mapper", online_url: "https://osu.ppy.sh/b/1001", star_rating: 6.1, difficulty: feature, base };
    const dynamicProfile = { target_star_rating: 6.1, candidate_min_section: 57, candidate_max_section: 65, stats_min_section: 57, stats_max_section: 65, sample_count: 842, mean: { aim: 0.5, speed: 0.5, reading: 0.5, slider: 0.5, overlap: 0.5 }, stddev: { aim: 0.12, speed: 0.12, reading: 0.12, slider: 0.12, overlap: 0.12 }, delta: { aim: 0.22, speed: 0.14, reading: 0.31, slider: -0.22, overlap: 0.07 }, z_score: { aim: 1.83, speed: 1.17, reading: 2.58, slider: 1.83, overlap: 0.58 }, weights: { aim: 1.63, speed: 1.13, reading: 2, slider: 1.63, overlap: 0.69 }, parameter_mean: { ar: 9, cs: 4, od: 8.4 }, parameter_stddev: { ar: 0.5, cs: 0.4, od: 0.6 }, parameter_delta: { ar: 0.2, cs: 0, od: 0.2 }, parameter_z_score: { ar: 0.4, cs: 0, od: 0.33 }, parameter_group_z_score: 0.3, parameter_weight: 0.48, fallback_reason: null };
    const results = [
      { ruleset: "osu" as const, beatmap_id: 2001, beatmapset_id: 601, artist: "Signal Garden", title: "Parallel Motion", version: "Another", creator: "Mapper A", online_url: "https://osu.ppy.sh/b/2001", star_rating: 6.0, difficulty: { ...feature, aim: 0.7, reading: 0.78 }, base: { ...base, bpm: 184 }, final_distance: 0.0462, difficulty_distance: 0.041, base_distance: 0.067 },
      { ruleset: "osu" as const, beatmap_id: 2002, beatmapset_id: 602, artist: "Night Circuit", title: "Crossing Lines", version: "Extra", creator: "Mapper B", online_url: "https://osu.ppy.sh/b/2002", star_rating: 6.3, difficulty: { ...feature, speed: 0.69, overlap: 0.61 }, base: { ...base, bpm: 192, ar: 9.4 }, final_distance: 0.0824, difficulty_distance: 0.074, base_distance: 0.116 },
      { ruleset: "osu" as const, beatmap_id: 2003, beatmapset_id: 603, artist: "Blue Window", title: "Readable Noise", version: "Expert", creator: "Mapper C", online_url: "https://osu.ppy.sh/b/2003", star_rating: 5.9, difficulty: { ...feature, slider: 0.34, reading: 0.75 }, base: { ...base, bpm: 178, length_seconds: 138 }, final_distance: 0.1197, difficulty_distance: 0.108, base_distance: 0.166 },
    ];
    if (command === "recommend_similar_beatmaps") {
      const request = args?.request as SimilarityRecommendationRequest | undefined;
      return { ruleset: "osu", kind: request?.kind ?? "recent", seed_count: 20, skipped_seed_count: 0, results: results.map((result) => ({ ...result, recommended_by: target })), dynamic_profiles: [{ ...dynamicProfile, seed_beatmap_id: target.beatmap_id }] } as T;
    }
    return { ruleset: "osu", target: { ...target, source: "index", analyzer_version: 4, normalization_version: 1 }, results, dynamic_profile: dynamicProfile } as T;
  }
  if (command === "update_settings") return args?.settings as T;
  if (command === "ignore_update_version") return {
    ...(browserPreviewValue<AppSettings>("get_settings") ?? {}),
    ignored_update_version: args?.version,
  } as T;
  if (["clear_profile_cache", "set_default_file_client", "set_display_gamma", "cancel_lazer_dedupe", "open_netease_music_search", "set_local_source", "reset_local_source", "start_tosu", "stop_tosu", "set_tosu_executable", "set_tosu_lyrics_executable", "cancel_online_beatmap_download", "begin_collection_task", "cancel_collection_task", "exit_app"].includes(command)) return null as T;
  return undefined;
}

export const desktopApi = {
  getAuthStatus: () => call<AuthStatus>("get_auth_status"),
  getBeatmapHubAuthStatus: () => call<BeatmapHubAuthStatus>("get_beatmaphub_auth_status"),
  createBeatmapHubProfile: (displayName: string, deviceName: string) =>
    call<BeatmapHubAuthStatus>("create_beatmaphub_profile", { displayName, deviceName }),
  loginBeatmapHub: () => call<BeatmapHubAuthStatus>("login_beatmaphub"),
  linkBeatmapHubDevice: (linkToken: string, deviceName: string) =>
    call<BeatmapHubAuthStatus>("link_beatmaphub_device", { linkToken, deviceName }),
  logoutBeatmapHub: () => call<void>("logout_beatmaphub"),
  getBeatmapHubProfile: () => call<BeatmapHubProfile>("get_beatmaphub_profile"),
  createBeatmapHubDeviceLink: () => call<BeatmapHubDeviceLink>("create_beatmaphub_device_link"),
  revokeBeatmapHubDevice: (deviceId: string) => call<void>("revoke_beatmaphub_device", { deviceId }),
  getBeatmapHubPack: (shareId: string) => call<BeatmapHubPack>("get_beatmaphub_pack", { shareId }),
  getBeatmapHubRecommendations: (limit = 20, forceRefresh = false) => call<BeatmapHubRecommendation[]>("get_beatmaphub_recommendations", { limit, forceRefresh }),
  searchBeatmapHubPacks: (query: string, limit = 20) => call<BeatmapHubRecommendation[]>("search_beatmaphub_packs", { query, limit }),
  previewBeatmapHubPack: (shareId: string) => call<BeatmapHubPackPreview>("preview_beatmaphub_pack", { shareId }),
  publishBeatmapHubPack: (folderId: string, title: string, description: string, isPrivate = false) =>
    call<BeatmapHubPublishResult>("publish_beatmaphub_pack", { folderId, title, description, isPrivate }),
  updateBeatmapHubPack: (shareId: string, folderId: string, title: string, description: string, isPrivate = false) =>
    call<void>("update_beatmaphub_pack", { shareId, folderId, title, description, isPrivate }),
  deleteBeatmapHubPack: (shareId: string) => call<void>("delete_beatmaphub_pack", { shareId }),
  rateBeatmapHubPack: (shareId: string, score: number) => call<void>("rate_beatmaphub_pack", { shareId, score }),
  favoriteBeatmapHubPack: (shareId: string, enabled: boolean) => call<void>("favorite_beatmaphub_pack", { shareId, enabled }),
  likeBeatmapHubPack: (shareId: string, enabled: boolean) => call<void>("like_beatmaphub_pack", { shareId, enabled }),
  getBeatmapHubComments: (shareId: string, limit = 50) => call<BeatmapHubComment[]>("get_beatmaphub_comments", { shareId, limit }),
  createBeatmapHubComment: (shareId: string, content: string) => call<BeatmapHubComment>("create_beatmaphub_comment", { shareId, content }),
  updateBeatmapHubComment: (commentId: string, content: string) => call<BeatmapHubComment>("update_beatmaphub_comment", { commentId, content }),
  deleteBeatmapHubComment: (commentId: string) => call<void>("delete_beatmaphub_comment", { commentId }),
  importBeatmapHubPack: (shareId: string, resolved: OnlineBeatmapset[]) =>
    call<BeatmapHubImportResult>("import_beatmaphub_pack", { shareId, resolved }),
  getCapabilities: () => call<PlatformCapabilities>("get_capabilities"),
  getLazerDiskUsage: () => call<LazerDiskUsage>("get_lazer_disk_usage"),
  dedupeLazerFiles: (dryRun: boolean) =>
    call<LazerDedupeResult>("dedupe_lazer_files", { dryRun }),
  cancelLazerDedupe: () => call<void>("cancel_lazer_dedupe"),

  inspectBeatmapPreview: (bid: number) =>
    call<BeatmapPreviewInspection>("inspect_beatmap_preview", { bid }),
  generateBeatmapPreview: (request: BeatmapPreviewRequest) =>
    call<BeatmapPreviewResult>("generate_beatmap_preview", { request }),
  readBeatmapPreviewOutput: (path: string) =>
    call<ArrayBuffer>("read_beatmap_preview_output", { path }),
  saveBeatmapPreviewOutput: (source: string, destination: string) =>
    call<string>("save_beatmap_preview_output", { source, destination }),
  openBeatmapPreviewOutput: (path: string) =>
    call<void>("open_beatmap_preview_output", { path }),

  saveOAuthCredentials: (clientId: string, clientSecret: string) =>
    call<{ client_id: string; callback_url: string }>(
      "save_oauth_credentials",
      { clientId, clientSecret },
    ),
  beginOAuthLogin: () => call<PendingOAuth>("begin_oauth_login"),
  cancelOAuthLogin: () => call<void>("cancel_oauth_login"),
  disconnectOsu: (revoke = true) =>
    call<DisconnectResult>("disconnect_osu", { revoke }),
  getOwnProfile: (ruleset: Ruleset, forceRefresh = false) =>
    call<Cached<OwnProfile>>("get_own_profile", {
      ruleset,
      forceRefresh,
    }),
  getOsekaiMedals: () => call<{ content?: unknown[] }>("get_osekai_medals"),
  getOsekaiMedalDetail: (medalId: number) => call<{ content?: unknown[] }>("get_osekai_medal_detail", { medalId }),
  getOsekaiMedalBeatmaps: (medalId: number) => call<{ content?: unknown[] }>("get_osekai_medal_beatmaps", { medalId }),
  getScores: (ruleset: Ruleset, category: ScoreCategory, offset = 0, limit = 100, forceRefresh = false) =>
    call<Cached<Score[]>>("get_scores", {
      ruleset,
      category,
      offset,
      limit,
      forceRefresh,
    }),
  searchOnlineBeatmapsets: (query: OnlineBeatmapSearchQuery) =>
    call<OnlineBeatmapSearchResponse>("search_online_beatmapsets", { query }),
  collectOnlineBeatmapsets: (
    query: OnlineBeatmapSearchQuery,
    limit: number,
  ) =>
    call<CollectedBeatmapsets>("collect_online_beatmapsets", { query, limit }),
  getOnlineBeatmapset: (beatmapsetId: number) =>
    call<OnlineBeatmapset>("get_online_beatmapset", { beatmapsetId }),
  getOnlineBeatmap: (beatmapId: number) =>
    call<Record<string, unknown>>("get_online_beatmap", { beatmapId }),
  getOnlineBeatmapProviderStatus: () =>
    call<BeatmapSourceStatus[]>("get_online_beatmap_provider_status"),
  calculateBeatmapPp: (request: BeatmapCalculationRequest) =>
    call<BeatmapCalculationResult>("calculate_beatmap_pp", { request }),
  downloadOnlineBeatmapsets: (request: BeatmapDownloadRequest) =>
    call<BeatmapDownloadResult>("download_online_beatmapsets", { request }),
  cancelOnlineBeatmapDownload: () =>
    call<void>("cancel_online_beatmap_download"),
  listCollections: () => call<CollectionSnapshot>("list_collections"),
  getCollectionSyncStatus: () => call<CollectionSyncStatus>("get_collection_sync_status"),
  refreshCollections: (client: OsuClient) => call<CollectionSnapshot>("refresh_collections", { client }),
  createCollection: (name: string, creator: string) => call<CollectionFolder>("create_collection", { name, creator }),
  renameCollection: (folderId: string, name: string) => call<void>("rename_collection", { folderId, name }),
  deleteCollection: (folderId: string) => call<void>("delete_collection", { folderId }),
  addCollectionEntries: (folderId: string, candidates: CollectionCandidate[]) => call<void>("add_collection_entries", { folderId, candidates }),
  removeCollectionEntry: (folderId: string, entryId: string) => call<void>("remove_collection_entry", { folderId, entryId }),
  writeStableCollections: () => call<CollectionWriteResult>("write_stable_collections"),
  getCollectionManagerStatus: () => call<CollectionManagerStatus>("get_collection_manager_status"),
  setCollectionManagerPath: (path: string | null) => call<void>("set_collection_manager_path", { path }),
  getCollectionBackupStatus: (client: OsuClient) => call<CollectionBackupStatus>("get_collection_backup_status", { client }),
  createCollectionBackup: (client: OsuClient) => call<CollectionBackupStatus>("create_collection_backup", { client }),
  writeLazerCollections: () => call<CollectionWriteResult>("write_lazer_collections"),
  restoreCollectionBackup: (client: OsuClient, backupPath: string) => call<void>("restore_collection_backup", { client, backupPath }),
  exportCollectionShare: (folderId: string, creator: string) => call<string>("export_collection_share", { folderId, creator }),
  previewCollectionShare: (code: string) => call<CollectionSharePreview>("preview_collection_share", { code }),
  importCollectionShare: (code: string) => call<CollectionFolder>("import_collection_share", { code }),
  importCollectionArchive: (path: string) => call<CollectionFolder>("import_collection_archive", { path }),
  getCollectionDownloadItems: (folderIds: string[]) => call<CollectionDownloadItem[]>("get_collection_download_items", { folderIds }),
  beginCollectionTask: () => call<void>("begin_collection_task"),
  cancelCollectionTask: () => call<void>("cancel_collection_task"),
  installCollectionDownloads: (folderIds: string[], archivePaths: string[]) =>
    call<CollectionInstallResult>("install_collection_downloads", { folderIds, archivePaths }),
  openCollectionDownloads: (archivePaths: string[]) =>
    call<CollectionOpenResult>("open_collection_downloads", { archivePaths }),
  openDownloadedPath: (path: string) => call<void>("open_downloaded_path", { path }),
  exitApp: () => call<void>("exit_app"),
  clearProfileCache: () => call<void>("clear_profile_cache"),
  checkForUpdates: () => call<UpdateCheckResult>("check_for_updates"),
  downloadAndInstallUpdate: (expectedVersion: string) =>
    call<void>("download_and_install_update", { expectedVersion }),
  ignoreUpdateVersion: (version: string) =>
    call<AppSettings>("ignore_update_version", { version }),
  getSettings: () => call<AppSettings>("get_settings"),
  updateSettings: (settings: AppSettings) =>
    call<AppSettings>("update_settings", { settings }),
  markOnboardingSeen: (version: number) =>
    call<AppSettings>("mark_onboarding_seen", { version }),
  markPageOnboardingSeen: (pageId: string, version: number) =>
    call<AppSettings>("mark_page_onboarding_seen", { pageId, version }),
  startGameSession: (ruleset: Ruleset, client: OsuClient, launchTosu?: boolean) =>
    call<GameSessionSummary>("start_game_session", { ruleset, client, launchTosu }),
  startDetectedGameSession: (ruleset: Ruleset, client: OsuClient) =>
    call<GameSessionSummary>("start_detected_game_session", { ruleset, client }),
  getGameSessionStatus: () =>
    call<GameSessionSummary | null>("get_game_session_status"),
  getGameStatus: () => call<GameStatusSnapshot>("get_game_status"),
  convertManiaBeatmaps: (paths: string[]) =>
    call<ManiaConversionResult>("convert_mania_beatmaps", { paths }),
  listGameMedia: (client: OsuClient) => call<GameMediaItem[]>("list_game_media", { client }),
  readGameReplay: (client: OsuClient, path: string) =>
    call<GameReplayPayload>("read_game_replay", { client, path }),
  inspectGameReplay: (client: OsuClient, path: string) =>
    call<ReplayMapInfo>("inspect_game_replay", { client, path }),
  readGameScreenshot: (client: OsuClient, path: string) =>
    call<GameScreenshotPayload>("read_game_screenshot", { client, path }),
  submitReplayRender: (request: ReplayRenderRequest) =>
    call<ReplayRenderJob>("submit_replay_render", { request }),
  getDanserStatus: () => call<DanserStatus>("get_danser_status"),
  listDanserProfiles: () => call<string[]>("list_danser_profiles"),
  enqueueDanserRenders: (request: DanserEnqueueRequest) =>
    call<DanserRenderJob[]>("enqueue_danser_renders", { request }),
  startDanserRenderQueue: () => call<void>("start_danser_render_queue"),
  getDanserRenderQueue: () => call<DanserRenderJob[]>("get_danser_render_queue"),
  cancelDanserRender: (id: string) => call<void>("cancel_danser_render", { id }),
  openDanserOutput: (path: string) => call<void>("open_danser_output", { path }),
  openMediaInExplorer: (client: OsuClient, path: string) =>
    call<void>("open_media_in_explorer", { client, path }),
  openLocalResourceInExplorer: (client: OsuClient, logicalPath: string) =>
    call<void>("open_local_resource_in_explorer", { client, logicalPath }),
  getDefaultFileClients: () => call<DefaultFileClients>("get_default_file_clients"),
  setDefaultFileClient: (kind: "beatmap" | "skin", client: OsuClient) =>
    call<void>("set_default_file_client", { kind, client }),
  setDisplayGamma: (gamma: number) => call<void>("set_display_gamma", { gamma }),
  openNeteaseMusicSearch: (artist: string, title: string) =>
    call<void>("open_netease_music_search", { artist, title }),
  generateTrainerBeatmap: (request: TrainerRequest) =>
    call<TrainerResult>("generate_trainer_beatmap", { request }),
  getTosuStatus: () => call<TosuStatus>("get_tosu_status"),
  getTosuLogs: () => call<TosuLogEntry[]>("get_tosu_logs"),
  setTosuExecutable: (path: string) => call<TosuStatus>("set_tosu_executable", { path }),
  setTosuLyricsExecutable: (path: string) => call<TosuStatus>("set_tosu_lyrics_executable", { path }),
  startTosu: () => call<void>("start_tosu"),
  stopTosu: () => call<void>("stop_tosu"),
  getObsStatus: () => call<ObsStatus>("get_obs_status"),
  getObsScenes: () => call<string[]>("get_obs_scenes"),
  saveObsConnection: (websocketUrl: string, password: string | null, selectedScene: string | null) => call<ObsStatus>("save_obs_connection", { websocketUrl, password, selectedScene }),
  refreshSelectedObsScene: () => call<ObsRefreshResult>("refresh_selected_obs_scene"),
  getLocalSources: () =>
    call<LocalSourceStatus[]>("get_local_sources"),
  getLocalIndexStatus: () =>
    call<LocalIndexLoadStatus>("get_local_index_status"),
  getSimilarityIndexStatus: (ruleset: Ruleset) =>
    call<SimilarityIndexStatus>("get_similarity_index_status", { ruleset }),
  configureSimilarityIndex: (ruleset: Ruleset, directory: string | null) =>
    call<SimilarityIndexStatus>("configure_similarity_index", { ruleset, directory }),
  querySimilarBeatmaps: (request: SimilarityQueryRequest) =>
    call<SimilarityQueryResponse>("query_similar_beatmaps", { request }),
  recommendSimilarBeatmaps: (request: SimilarityRecommendationRequest) =>
    call<SimilarityRecommendationResponse>("recommend_similar_beatmaps", { request }),
  setLocalSource: (client: OsuClient, path: string) =>
    call<LocalSourceStatus>("set_local_source", { client, path }),
  resetLocalSource: (client: OsuClient) =>
    call<LocalSourceStatus>("reset_local_source", { client }),
  getLocalSummary: (client: OsuClient) =>
    call<LocalLibrarySummary | null>("get_local_summary", { client }),
  scanLocalSource: (client: OsuClient, force = false) =>
    call<LocalLibrarySummary>("scan_local_source", { client, force }),
  cancelLocalScan: (client: OsuClient) =>
    call<void>("cancel_local_scan", { client }),
  queryLocalBeatmaps: (query: BeatmapQuery) =>
    call<Page<LocalBeatmapSummary>>("query_local_beatmaps", { query }),
  queryLocalBeatmapSets: (query: BeatmapQuery) =>
    call<Page<LocalBeatmapSetSummary>>("query_local_beatmap_sets", { query }),
  getLocalBeatmapDetail: (client: OsuClient, resourceId: string) =>
    call<LocalBeatmapDetail>("get_local_beatmap_detail", {
      client,
      resourceId,
    }),
  getLocalBeatmapPath: (client: OsuClient, resourceId: string) =>
    call<string>("get_local_beatmap_path", { client, resourceId }),
  getLocalBeatmapBackground: (client: OsuClient, resourceId: string) =>
    call<string | null>("get_local_beatmap_background", {
      client,
      resourceId,
    }),
  queryLocalSkins: (query: SkinQuery) =>
    call<Page<LocalSkinSummary>>("query_local_skins", { query }),
  getLocalSkinDetail: (client: OsuClient, resourceId: string) =>
    call<LocalSkinDetail>("get_local_skin_detail", { client, resourceId }),
  exportLocalBeatmapSet: (client: OsuClient, setKey: string, outDir: string) =>
    call<string>("export_local_beatmap_set", { client, setKey, outDir }),
  exportLocalSkin: (client: OsuClient, resourceId: string, outDir: string) =>
    call<string>("export_local_skin", { client, resourceId, outDir }),
  getLocalSkinPreview: (client: OsuClient, resourceId: string) =>
    call<LocalSkinPreview>("get_local_skin_preview", { client, resourceId }),
  getLocalSkinAsset: (
    client: OsuClient,
    skinResourceId: string,
    assetResourceId: string,
  ) =>
    call<LocalSkinAssetPayload>("get_local_skin_asset", {
      client,
      skinResourceId,
      assetResourceId,
    }),
  replaceLocalSkinAsset: (client: OsuClient, skinResourceId: string, assetResourceId: string, replacementPath: string, saveAsNew: boolean, newSkinName?: string) =>
    call<void>("replace_local_skin_asset", { client, skinResourceId, assetResourceId, replacementPath, saveAsNew, newSkinName: newSkinName ?? null }),
  chooseSkinAssetFile: async (extension: string) => {
    if (!isTauri()) throw { code: "TAURI_REQUIRED", message: "文件选择器仅可在 OPP 桌面应用中使用" } satisfies CommandError;
    const selected = await openDialog({ multiple: false, title: `选择 .${extension} 替换文件`, filters: [{ name: `${extension.toUpperCase()} 文件`, extensions: [extension] }] });
    return typeof selected === "string" ? selected : null;
  },
  openSkinWorkshopPackage: (path: string) =>
    call<LocalSkinSummary>("open_skin_workshop_package", { path }),
  chooseSkinWorkshopPackage: async () => {
    if (!isTauri()) return null;
    const selected = await openDialog({ multiple: false, title: "打开 Skin 安装包", filters: [{ name: "osu! Skin", extensions: ["osk"] }] });
    return typeof selected === "string" ? selected : null;
  },
  getSkinWorkshopTree: (client: OsuClient, skinResourceId: string) =>
    call<SkinTree>("get_skin_workshop_tree", { client, skinResourceId }),
  getSkinWorkshopPartPreview: (client: OsuClient, skinResourceId: string, partKey: string) =>
    call<SkinPartPreview>("get_skin_workshop_part_preview", { client, skinResourceId, partKey }),
  getSkinWorkshopAsset: (client: OsuClient, skinResourceId: string, assetId: string) =>
    call<SkinWorkshopAssetPayload>("get_skin_workshop_asset", { client, skinResourceId, assetId }),
  getSkinWorkshopConfig: (client: OsuClient, skinResourceId: string) =>
    call<SkinConfigDocument>("get_skin_workshop_config", { client, skinResourceId }),
  executeSkinWorkshopAction: (targetSkinResourceId: string, mode: SkinWorkshopWriteMode, action: SkinWorkshopAction) =>
    call<SkinWorkshopMutationResult>("execute_skin_workshop_action", { targetSkinResourceId, mode, action }),
  executeSkinWorkshopPreset: (targetSkinResourceId: string, mode: SkinWorkshopWriteMode, preset: SkinWorkshopPreset) =>
    call<SkinWorkshopMutationResult>("execute_skin_workshop_preset", { targetSkinResourceId, mode, preset }),
  chooseLocalDirectory: async (defaultPath?: string | null) => {
    if (!isTauri()) {
      throw {
        code: "TAURI_REQUIRED",
        message: "目录选择器仅在 OPP 桌面应用中可用",
      } satisfies CommandError;
    }
    const selected = await openDialog({
      directory: true,
      multiple: false,
      defaultPath: defaultPath ?? undefined,
      title: "选择 osu! 本地目录",
    });
    return typeof selected === "string" ? selected : null;
  },
  chooseBeatmapPreviewDestination: async (fileName: string, extension: "gif" | "png") => {
    if (!isTauri()) return null;
    const selected = await saveDialog({
      title: "保存铺面预览",
      defaultPath: fileName,
      filters: [{ name: extension === "gif" ? "GIF 动图" : "PNG 图片", extensions: [extension] }],
    });
    return typeof selected === "string" ? selected : null;
  },
  chooseDanserExecutable: async (defaultPath?: string | null) => {
    if (!isTauri()) return null;
    const isWindows = navigator.userAgent.includes("Windows");
    const selected = await openDialog({
      directory: false,
      multiple: false,
      defaultPath: defaultPath ?? undefined,
      title: isWindows ? "选择 danser-cli.exe" : "选择 danser 可执行文件",
      ...(isWindows ? { filters: [{ name: "Danser CLI", extensions: ["exe"] }] } : {}),
    });
    return typeof selected === "string" ? selected : null;
  },
  chooseDirectory: async (title: string, defaultPath?: string | null) => {
    if (!isTauri()) {
      return defaultPath ?? "C:/OPP-preview";
    }
    const selected = await openDialog({ directory: true, multiple: false, defaultPath: defaultPath ?? undefined, title });
    return typeof selected === "string" ? selected : null;
  },
  chooseSimilarityBeatmapFile: async () => {
    if (!isTauri()) return "C:/OPP-preview/reference.osu";
    const selected = await openDialog({
      multiple: false,
      title: "选择参考谱面",
      filters: [{ name: "osu! beatmap", extensions: ["osu"] }],
    });
    return typeof selected === "string" ? selected : null;
  },
  chooseTosuExecutable: async (defaultPath?: string | null) => {
    if (!isTauri()) throw { code: "TAURI_REQUIRED", message: "文件选择器仅可在 OPP 桌面应用中使用" } satisfies CommandError;
    const isWindows = navigator.userAgent.includes("Windows");
    const selected = await openDialog({
      multiple: false,
      defaultPath: defaultPath ?? undefined,
      title: isWindows ? "选择 tosu.exe" : "选择 tosu 可执行文件",
      ...(isWindows ? { filters: [{ name: "tosu", extensions: ["exe"] }] } : {}),
    });
    return typeof selected === "string" ? selected : null;
  },
  chooseTosuLyricsExecutable: async (defaultPath?: string | null) => {
    if (!isTauri()) throw { code: "TAURI_REQUIRED", message: "文件选择器仅可在 OPP 桌面应用中使用" } satisfies CommandError;
    const isWindows = navigator.userAgent.includes("Windows");
    const selected = await openDialog({
      multiple: false,
      defaultPath: defaultPath ?? undefined,
      title: isWindows ? "选择 tosu-proxy.exe" : "选择 tosu-proxy 可执行文件",
      ...(isWindows ? { filters: [{ name: "tosu-lyrics", extensions: ["exe"] }] } : {}),
    });
    return typeof selected === "string" ? selected : null;
  },
  chooseManiaBeatmaps: async () => {
    if (!isTauri()) throw { code: "TAURI_REQUIRED", message: "文件选择器仅可在 OPP 桌面应用中使用" } satisfies CommandError;
    const selected = await openDialog({ multiple: true, title: "选择 Malody 谱面", filters: [{ name: "Malody chart", extensions: ["mcz"] }] });
    return Array.isArray(selected) ? selected : typeof selected === "string" ? [selected] : [];
  },
  exportReplayVideo: (videoUrl: string, fileName: string) =>
    call<string>("export_replay_video", { videoUrl, fileName }),
  chooseBeatmapDownloadDirectory: async (defaultPath?: string | null) => {
    if (!isTauri()) {
      throw {
        code: "TAURI_REQUIRED",
        message: "目录选择器仅可在 OPP 桌面应用中使用",
      } satisfies CommandError;
    }
    const selected = await openDialog({
      directory: true,
      multiple: false,
      defaultPath: defaultPath ?? undefined,
      title: "选择谱面下载目录",
    });
    return typeof selected === "string" ? selected : null;
  },
  openExternal: async (url: string) => {
    if (isTauri()) await openUrl(url);
    else window.open(url, "_blank", "noopener,noreferrer");
  },
  onOAuthResult: async (
    handler: (result: OAuthResult) => void,
  ): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<OAuthResult>("oauth-result", (event) => handler(event.payload));
  },
  onUpdateProgress: async (
    handler: (progress: UpdateProgress) => void,
  ): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<UpdateProgress>("update-progress", (event) => handler(event.payload));
  },
  onLocalScanProgress: async (
    handler: (progress: LocalScanProgress) => void,
  ): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<LocalScanProgress>("local-scan-progress", (event) =>
      handler(event.payload),
    );
  },
  onLazerDedupeProgress: async (
    handler: (progress: LazerDedupeProgress) => void,
  ): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<LazerDedupeProgress>("lazer-dedupe-progress", (event) =>
      handler(event.payload),
    );
  },
  onBeatmapDownloadProgress: async (
    handler: (progress: BeatmapDownloadProgress) => void,
  ): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<BeatmapDownloadProgress>(
      "beatmap-download-progress",
      (event) => handler(event.payload),
    );
  },
  onCollectionTaskProgress: async (
    handler: (progress: CollectionTaskProgress) => void,
  ): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<CollectionTaskProgress>("collection-task-progress", (event) =>
      handler(event.payload),
    );
  },
  onReplayRenderProgress: async (
    handler: (progress: ReplayRenderProgress) => void,
  ): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<ReplayRenderProgress>("ordr-render-progress", (event) =>
      handler(event.payload),
    );
  },
  liveRenderOpen: (
    beatmapPath: string,
    replayPath: string,
    options: LiveRenderOptions,
    rect: { x: number; y: number; width: number; height: number },
  ) =>
    call<{ durationMs: number }>("live_render_open", {
      beatmapPath,
      replayPath,
      options,
      rect,
    }),
  liveRenderMove: (rect: { x: number; y: number; width: number; height: number; suppressed?: boolean }) =>
    call<void>("live_render_move", { rect }),
  liveRenderSeek: (timeMs: number) => call<void>("live_render_seek", { timeMs }),
  liveRenderSetOptions: (options: LiveRenderOptions) =>
    call<void>("live_render_set_options", { options }),
  liveRenderCheckFfmpeg: () => call<string | null>("live_render_check_ffmpeg"),
  liveRenderListSkins: (client: OsuClient) => call<LiveSkinEntry[]>("live_render_list_skins", { client }),
  liveRenderCheckNvenc: () => call<[boolean, boolean]>("live_render_check_nvenc"),
  liveRenderGetFfmpegStatus: () => call<FfmpegStatusInfo>("live_render_get_ffmpeg_status"),
  chooseFfmpegExecutable: async (defaultPath?: string | null) => {
    if (!isTauri()) return null;
    const isWindows = navigator.userAgent.includes("Windows");
    const selected = await openDialog({
      directory: false,
      multiple: false,
      defaultPath: defaultPath ?? undefined,
      title: isWindows ? "选择 ffmpeg.exe" : "选择 ffmpeg 可执行文件",
      ...(isWindows ? { filters: [{ name: "FFmpeg", extensions: ["exe"] }] } : {}),
    });
    return typeof selected === "string" ? selected : null;
  },
  liveRenderExport: (
    beatmapPath: string,
    replayPath: string,
    options: LiveRenderOptions,
    params: LiveExportParams,
  ) =>
    call<string>("live_render_export", {
      beatmapPath,
      replayPath,
      options,
      params,
    }),
  liveRenderExportCancel: () => call<void>("live_render_export_cancel"),
  liveRenderOpenExportOutput: (path: string) =>
    call<void>("live_render_open_export_output", { path }),
  onLiveRenderExport: async (
    handler: (progress: { phase: string; frame: number; total: number; message: string }) => void,
  ): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<{
      phase: string;
      frame: number;
      total: number;
      message: string;
    }>("live-render-export", (event) => handler(event.payload));
  },
  liveRenderPlay: () => call<void>("live_render_play"),
  liveRenderPause: () => call<void>("live_render_pause"),
  liveRenderClose: () => call<void>("live_render_close"),
  onLiveRenderTime: async (
    handler: (state: { active: boolean; playing: boolean; timeMs: number; durationMs: number }) => void,
  ): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<{
      active: boolean;
      playing: boolean;
      time_ms: number;
      duration_ms: number;
    }>("live-render-time", (event) =>
      handler({
        active: event.payload.active,
        playing: event.payload.playing,
        timeMs: event.payload.time_ms,
        durationMs: event.payload.duration_ms,
      }),
    );
  },
  /** 皮肤热切换失败(加载错误;当前皮肤保持不变)。 */
  onLiveRenderSkinError: async (handler: (message: string) => void): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<string>("live-render-skin-error", (event) => handler(event.payload));
  },
  /** 渲染线程异常:会话已被后端清理(停播 + 销毁窗口),UI 需复位。 */
  onLiveRenderError: async (handler: (message: string) => void): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<string>("live-render-error", (event) => handler(event.payload));
  },
  onDanserRenderProgress: async (
    handler: (progress: DanserRenderJob) => void,
  ): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<DanserRenderJob>("danser-render-progress", (event) => handler(event.payload));
  },
  onNewReplaysDetected: async (
    handler: (payload: NewReplaysDetected) => void,
  ): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<NewReplaysDetected>("new-replays-detected", (event) => handler(event.payload));
  },
  onGameStatusChanged: async (
    handler: (status: GameStatusSnapshot) => void,
  ): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<GameStatusSnapshot>("game-status-changed", (event) => handler(event.payload));
  },
  onTosuLog: async (handler: (entry: TosuLogEntry) => void): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<TosuLogEntry>("tosu-log", (event) => handler(event.payload));
  },
  onTosuLiveData: async (handler: (snapshot: TosuLiveSnapshot) => void): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<TosuLiveSnapshot>("tosu-live-data", (event) => handler(event.payload));
  },
  onObsStatusChanged: async (handler: (status: ObsStatus) => void): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<ObsStatus>("obs-status-changed", (event) => handler(event.payload));
  },
};

export const capabilitiesQueryKey = ["platform-capabilities"] as const;

/**
 * 平台能力（由后端 `get_capabilities` 用 `cfg` 一次性算好下发）。业务页面面向
 * 能力判断，而非直接判断操作系统 —— 新增平台只改后端 `platform` 模块。
 */
export function useCapabilities() {
  return useQuery({
    queryKey: capabilitiesQueryKey,
    queryFn: () => desktopApi.getCapabilities(),
    staleTime: Infinity,
    retry: false,
  });
}

export interface LiveExportParams {
  outPath: string;
  width: number;
  height: number;
  fps: number;
  encoder: "x264" | "x265" | "nvenc" | "hevc_nvenc";
  quality: number;
  audio: boolean;
  hitsounds: boolean;
  /** 导出专用 BGM 偏移 ms(与预览偏移独立,默认 0)。 */
  audioOffset: number;
}

export interface LiveRenderOptions {
  urBar: boolean;
  followPoints: boolean;
  keyOverlay: boolean;
  /** 实时 PP 计数器(逐物件渐增,Argon 样式挂 ACC 行下方;后端字段 ppDisplay)。 */
  ppDisplay: boolean;
  bg: boolean;
  bgOpacity: number;
  audio: boolean;
  audioOffset: number;
  hitsounds: boolean;
  /** 光标尺寸倍率 0.1..2(lazer GameplayCursorSize,默认 1)。 */
  cursorSize: number;
  /** 用户皮肤目录路径;null = 内置 Argon-Pro(后端字段 skinPath)。 */
  skinPath: string | null;
  /** 强制用皮肤 combo 色覆盖谱面 [Colours](stable 行为,默认关:谱面色优先)。 */
  skinColours: boolean;
}

export interface LiveSkinEntry {
  name: string;
  path: string;
}
