import type { BeatmapQuery, OsuClient } from "./osu";

export type MusicMode = "sequential" | "repeat_all" | "repeat_one" | "shuffle";
export interface MusicTrack { id: string; title: string; artist: string; clients: OsuClient[] }
export interface MusicState {
  version: number; queue_version: number; current: MusicTrack | null; resource_id: string | null;
  playing: boolean; position: number; duration: number; volume: number; mode: MusicMode;
  total: number; notice: string | null; mini: boolean; background_tasks: number;
}
export interface MusicQueueRequest {
  client?: OsuClient; query?: BeatmapQuery; collection_id?: string; resource_id?: string;
  append?: boolean; preview?: boolean;
}
export type MusicControl = { action: "play" | "pause" | "toggle" | "next" | "previous" | "clear" }
  | { action: "seek"; seconds: number } | { action: "volume"; value: number }
  | { action: "mode"; mode: MusicMode } | { action: "select" | "remove"; id: string };
export interface MusicQueuePage { items: MusicTrack[]; total: number; version: number }
export interface MusicWindowSession { route: string; pinned: boolean | null }
export interface MusicLocation { client: OsuClient; ruleset: import("./osu").Ruleset; set_key: string; resource_id: string }
