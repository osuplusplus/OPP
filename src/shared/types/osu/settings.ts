// 从 types/osu.ts 按域拆分；types/osu.ts 是对外的 barrel，消费方导入路径不变。
import type { CommandError, OsuClient, ThemeColor, ThemeMode } from "./common";
import type { BeatmapDownloadProvider } from "./localResources";
import type { DanserRenderPreferences } from "./media";
import type { DifficultyFeatureVector } from "./similarity";

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

export interface LocalDatabaseStatus {
  phase: "unconfigured" | "ready" | "error";
  directory: string | null;
  recommended_directory: string;
  database_uuid: string | null;
  schema_version: number | null;
  can_initialize: boolean;
  error: CommandError | null;
}

export interface AppSettings {
  show_local_beatmap_presence?: boolean;
  local_beatmap_presence_scope?: "all" | OsuClient;
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
  ffmpeg_executable_path?: string | null;
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
  otd_executable_path?: string | null;
  launch_otd_with_game?: boolean;
  overlay_enabled?: boolean;
  overlay_show_with_game?: boolean;
  overlay_auto_hide_on_game_exit?: boolean;
  overlay_corner?: "TopLeft" | "TopRight" | "BottomLeft" | "BottomRight";
  overlay_hotkey_toggle?: string;
  overlay_click_through?: boolean;
  overlay_mp_auto_download?: boolean;
  overlay_start_tosu_with_game?: boolean;
  overlay_ingame_enabled?: boolean;
  overlay_irc_username?: string | null;
  /** 聊天翻译服务：google_free | tencent | baidu | openai | off */
  translation_provider?: string;
  /** 是否自动翻译收到的 MP 聊天消息。 */
  translation_enabled?: boolean;
  /** 入站翻译目标语言（内部语言码，如 zh）。 */
  translation_target_lang?: string;
  /** 出站翻译目标语言（自己要发的消息翻译成什么语言）。 */
  translation_outgoing_target_lang?: string;
  /** OpenAI 兼容服务地址（可带或不带 /v1；凭据不入设置）。 */
  translation_api_base_url?: string | null;
  /** OpenAI 兼容服务模型名（如 glm-4-flash）。 */
  translation_api_model?: string | null;
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
