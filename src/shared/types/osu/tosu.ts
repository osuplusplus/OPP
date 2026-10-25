// 从 types/osu.ts 按域拆分；types/osu.ts 是对外的 barrel，消费方导入路径不变。
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
export type LogLevel = "debug" | "info" | "warning" | "error" | "critical";
export interface LogFileInfo { name: string; size_bytes: number; created_at: string; }

export interface OverlayState {
  enabled: boolean;
  visible: boolean;
  mode: "hidden" | "hud" | "panel";
}

export interface OverlayIrcStatus {
  state: "disconnected" | "connecting" | "connected" | "error" | string;
  nick: string;
  channels: string[];
  message: string;
}

export interface OverlayMpActivity {
  phase:
    | "idle"
    | "checking"
    | "missing"
    | "downloading"
    | "imported"
    | "present"
    | "unresolved"
    | "error"
    | string;
  title: string;
  message: string;
}

export interface OverlayMpStatus {
  irc: OverlayIrcStatus;
  mp: OverlayMpActivity;
  irc_configured: boolean;
  irc_username: string | null;
  ingame_installed: boolean;
}

export interface OverlayIngameInstallInfo {
  installed: boolean;
  tosu_dir: string;
  message: string;
}

/** MP 频道单条聊天消息；translation 由后端自动翻译任务异步回填。 */
export interface OverlayChatEntry {
  id: number;
  channel: string;
  nick: string;
  text: string;
  translation?: string;
  /** chat | system */
  kind: string;
  ts_ms: number;
}

/** overlay-chat-updated 事件载荷：按 id 定位回填译文。 */
export interface OverlayChatUpdate {
  id: number;
  channel: string;
  translation: string;
}

export interface TosuLiveSnapshot {
  state: string | null; mode: string | null; artist: string | null; title: string | null; difficulty: string | null;
  song_time_ms: number | null; song_length_ms: number | null;
  score: number | null; combo: number | null; max_combo: number | null; accuracy: number | null;
  misses: number | null; hit_300: number | null; hit_100: number | null; hit_50: number | null;
  pp_current: number | null; pp_fc: number | null; mods: string | null;
  beatmap_id: number | null; beatmapset_id: number | null; beatmap_md5: string | null;
}
