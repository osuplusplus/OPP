import type { AppSettings, CollectionPersonalRecord, LocalDatabaseStatus, SimilarityQueryRequest, SimilarityRecommendationRequest } from "../../types/osu";
import { communityPreview } from "../communityPreview";

// 浏览器预览模式的命令夹具：非 Tauri 环境下返回静态样例数据。
export function browserPreviewValue<T>(command: string, args?: Record<string, unknown>): T | undefined {
  const community = communityPreview(command, args);
  if (community !== undefined) return community as T;
  if (command === "get_local_library_storage_status" || command === "migrate_local_library_database") return ["stable", "lazer"].map((client) => ({ client, storage: "unscanned", revision: null, entry_count: 0, beatmap_count: 0, error: null })) as T;
  if (command === "query_local_beatmap_presence") return (args?.ids as number[] ?? []).map((beatmap_id) => ({ beatmap_id, status: "unknown", clients: [] })) as T;
  if (command === "get_local_database_status" || command === "retry_local_database") return {
    phase: "ready", directory: "/preview/datasets", recommended_directory: "/preview/datasets",
    database_uuid: "00000000-0000-4000-8000-000000000001", schema_version: 2, can_initialize: false, error: null,
  } satisfies LocalDatabaseStatus as T;
  if (command === "query_collection_browser") return { items: [], total: 0, offset: 0, limit: 50, matching_folders: [], pool: null } as T;
  if (command === "get_collection_artwork" || command === "get_collection_entry_scores") return [] as T;
  if (command === "refresh_local_scores") return { players: [], default_player: null, errors: [], count: 0 } as T;
  if (command === "save_collection_record") return { ...(args?.record as object), revision: Number((args?.record as CollectionPersonalRecord).revision) + 1 } as T;
  if (command === "get_collection_record") return { revision: 0, tags: [], note: "", slot_override: null, scores: [], representative: null } as T;
  if (command === "get_local_artwork_sample") return [] as T;
  if (command === "get_pending_tournament_link" || command === "acknowledge_tournament_link") return null as T;
  if (command === "get_capabilities") return { os: "windows", display_gamma: true, file_association: true } as T;
  if (command === "get_beatmaphub_recommendations") return [] as T;
  if (command === "list_collections" || command === "list_collection_summaries") return { folders: [], sources: [] } as T;
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
  if (command === "get_career_status") return { configured: true, path: "/preview/datasets/career.sqlite3", snapshot_count: 0, latest_date: null, last_error: null } as T;
  if (command === "get_career_calendar") return { ruleset: args?.ruleset ?? "osu", start_date: args?.startDate ?? "", end_date: args?.endDate ?? "", days: [] } as T;
  if (command === "get_career_day") return { ruleset: args?.ruleset ?? "osu", date: args?.date ?? "", status: "missing", captured_at: null, stats: null, previous_stats: null, error: null, score_diffs: [], medal_events: [], media_events: [] } as T;
  if (command === "capture_career_snapshot") return { local_date: new Date().toISOString().slice(0, 10), status: "captured", created: false, fetched_at: new Date().toISOString(), profile_available: true, scores_available: false, media_available: false, message: "预览模式不会写入生涯数据库" } as T;
  if (command === "get_online_beatmap_background") return null as T;
  if (command === "analyze_player_skills") return {
    player: { id: 10001, username: "Preview User", country_code: "CN", avatar_url: "https://a.ppy.sh/10001", avatar_data_url: null },
    ruleset: "osu",
    algorithm: { id: "opp-osuskills-standard", version: "1", source: "Public osuSkills model, clean Rust implementation" },
    skills: { stamina: 286, tenacity: 241, agility: 318, accuracy: 274, precision: 302, reaction: 219, memory: 96, reading: 337 },
    contributions: [1, 2, 3].map((id, index) => ({
      beatmap_id: 1000 + id, title: ["Signal Garden", "Night Circuit", "Blue Window"][index], artist: "Preview Artist", version: "Insane", creator: "Preview Mapper", mods: index === 1 ? ["HD", "DT"] : [], pp: 320 - index * 24, accuracy: 98.4 - index * 0.4, combo: 850 - index * 60, max_combo: 902, misses: index, source: index === 2 ? "online" : "local", resource_id: index === 2 ? null : `stable:beatmap:preview-${id}`, skills: { stamina: 260 + index * 8, tenacity: 220 + index * 12, agility: 310 - index * 14, accuracy: 280, precision: 300 - index * 9, reaction: 215 + index * 8, memory: index * 12, reading: 330 - index * 10 }, weighted_skills: { stamina: 250 + index * 5, tenacity: 210 + index * 10, agility: 295 - index * 12, accuracy: 280, precision: 288 - index * 8, reaction: 205 + index * 8, memory: index * 10, reading: 315 - index * 9 }, error: null,
    })),
    coverage: { requested_scores: 200, analyzed_scores: 184, reused_scores: 176, local_scores: 156, online_scores: 28, skipped_scores: 16, skipped_reasons: ["部分谱面暂时不可用"] },
    fetched_at: new Date().toISOString(), stale: false,
  } as T;
  if (command === "get_game_status") return { clients: [{ client: "stable", running: false, executable: null, detected_at: new Date().toISOString() }, { client: "lazer", running: false, executable: null, detected_at: new Date().toISOString() }] } as T;
  if (command === "get_game_session_status") return null as T;
  if (command === "get_local_sources") return [] as T;
  if (command === "choose_game_replay_files") return { items: [], failures: [] } as T;
  if (command === "list_game_media") return [] as T;
  if (command === "get_tosu_status") return { installed: false, executable_path: null, api_base_url: "http://127.0.0.1:24050", api_reachable: false, running: false, owned_by_opp: false, dashboard_url: "http://127.0.0.1:24050", last_error: null, lyrics: { installed: false, executable_path: null, running: false, owned_by_opp: false, proxy_url: "http://127.0.0.1:41280/lyrics/" } } as T;
  if (command === "get_obs_status") return { running: false, websocket_url: "ws://127.0.0.1:4455", connected: false, password_configured: false, selected_scene: null, last_error: null } as T;
  if (command === "get_obs_scenes") return ["直播场景", "练习场景"] as T;
  if (command === "refresh_selected_obs_scene") return { refreshed_sources: [], skipped: true, message: "预览模式未连接 OBS" } as T;
  if (command === "get_default_file_clients") return { beatmap: "stable", skin: "stable" } as T;
  if (command === "get_otd_status") return { installed: false, executable_path: null, daemon_running: false, owned_by_opp: false, version: null, tablet_name: null, config_path: null, config_summary: null, last_error: null } as T;
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
      const patternView = {
        category: "Shield",
        mode_tag: "Mix",
        coverage: [0.52, 0.31, 0.12, 0.66, 0.48, 0.09],
        bars: [
          { pattern: "Coordination", amount: 158_000, relative: 0.66, specific_types: [["Shield", 0.092], ["Chordjack", 0.041]] },
          { pattern: "Stream", amount: 121_000, relative: 0.51, specific_types: [["Light Stream", 0.28]] },
          { pattern: "Wildcard", amount: 239_000, relative: 1, specific_types: [] },
        ],
        subtypes: [["Shield", 0.092], ["Light Stream", 0.081]],
        ln_note_ratio: 0.234,
        intensity: [6.4, 12.1, 5.2, 18.5],
        temporal: [0.42, 0.18, 0.12],
        duration_seconds: 132,
        sv_amount: 0,
      };
      const target = { ruleset: "mania" as const, beatmap_id: 3001, beatmapset_id: 701, artist: "Synthetic Artist", title: "Key Reference", version: "4K Another", creator: "Preview Mapper", online_url: "https://osu.ppy.sh/beatmaps/3001", key_count: 4 as const, family: "rc" as const, pattern: "stream" as const, difficulty, style, base, difficulty_percentile: 0.78, difficulty_band: 7, game_mod: "target_mod" in similarityRequest ? similarityRequest.target_mod : similarityRequest.candidate_mods[0] ?? "NM", pattern_view: patternView };
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
  if (["clear_profile_cache", "clear_career_history", "set_default_file_client", "set_display_gamma", "cancel_lazer_dedupe", "cancel_video_fix", "open_netease_music_search", "set_local_source", "reset_local_source", "start_tosu", "stop_tosu", "set_tosu_executable", "set_tosu_lyrics_executable", "start_otd", "stop_otd", "set_otd_executable", "read_otd_config_summary", "open_otd", "cancel_online_beatmap_download", "begin_collection_task", "cancel_collection_task", "exit_app", "overlay_set_mode", "overlay_set_visible", "overlay_test_hotkey", "overlay_set_irc_credentials", "overlay_install_ingame", "overlay_uninstall_ingame"].includes(command)) return null as T;
  if (command === "overlay_get_state") return { enabled: false, visible: false, mode: "hud" } as T;
  if (command === "overlay_get_mp_status") return { irc: { state: "disconnected", nick: "", channels: [], message: "" }, mp: { phase: "idle", title: "", message: "" }, irc_configured: false, irc_username: null, ingame_installed: false } as T;
  if (command === "overlay_install_ingame") return { installed: false, tosu_dir: "", message: "" } as T;
  if (command === "backup_otd_config") return `${args?.destinationDir ?? "C:\\Export"}/OpenTabletDriver-backup.json` as T;
  return undefined;
}
