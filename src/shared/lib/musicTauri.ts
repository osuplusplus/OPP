// Kept separate from the full desktop adapter so the mini entry cannot pull in app features.
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { MusicControl, MusicLocation, MusicQueuePage, MusicQueueRequest, MusicState, MusicWindowSession } from "../types/music";
import type { AppSettings, CollectionSnapshot } from "../types/osu";

export const emptyMusicState: MusicState = { version: 0, queue_version: 0, current: null, resource_id: null, playing: false, position: 0, duration: 0, volume: .65, mode: "repeat_all", total: 0, notice: null, mini: false, background_tasks: 0 };
function call<T>(command: string, args?: Record<string, unknown>) {
  if (!isTauri()) return Promise.reject(new Error("请在 OPP 桌面应用中使用本地播放器"));
  return invoke<T>(command, args);
}
export const musicDesktop = {
  available: isTauri,
  frontendTask: (active: boolean) => isTauri() ? call<void>("music_frontend_task", { active }) : Promise.resolve(),
  state: () => isTauri() ? call<MusicState>("music_state") : Promise.resolve(emptyMusicState),
  location: (resourceId: string) => call<MusicLocation | null>("music_location", { resourceId }),
  queue: (offset: number, limit: number, search: string) => call<MusicQueuePage>("music_queue_page", { offset, limit, search }),
  control: (control: MusicControl) => call<void>("music_control", { control }),
  setQueue: (request: MusicQueueRequest) => call<void>("music_set_queue", { request }),
  artwork: (id: string) => call<string | null>("music_artwork", { id }),
  mode: (mini: boolean) => call<void>("music_window_mode", { mini, route: window.location.hash.slice(1) || "/local" }),
  ready: () => isTauri() ? call<MusicWindowSession>("music_window_ready") : Promise.resolve({ route: "", pinned: true }),
  layout: (expanded: boolean, pinned: boolean) => call<void>("music_mini_layout", { expanded, pinned }),
  drag: () => getCurrentWindow().startDragging(),
  hide: () => getCurrentWindow().hide(),
  exit: () => call<void>("exit_app"),
  settings: () => call<AppSettings>("get_settings"),
  collections: () => call<CollectionSnapshot>("list_collections"),
  subscribe: (callback: (state: MusicState) => void) => isTauri() ? listen<MusicState>("music-player-state", ({ payload }) => callback(payload)) : Promise.resolve(() => {}),
  onWindowError: (callback: (message: string) => void) => isTauri() ? listen<string>("music-window-error", ({ payload }) => callback(payload)) : Promise.resolve(() => {}),
};
