// 从 types/osu.ts 按域拆分；types/osu.ts 是对外的 barrel，消费方导入路径不变。
import type { CommandError, OsuClient, Ruleset } from "./common";

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
export interface ReplayFileSelection {
  items: GameMediaItem[];
  failures: Array<{ path: string; error: CommandError }>;
}

export interface ReplayMapInfo {
  ruleset: Ruleset;
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

export interface FfmpegStatusInfo {
  path: string | null;
  version: string | null;
  source: "manual" | "path" | "danser" | "";
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
