import type { ObsRefreshResult, ObsStatus, TosuLiveSnapshot, TosuLogEntry, TosuStatus } from "../../types/osu";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { call, isTauri } from "./invoke";

// liveIntegration 域的 Tauri 命令适配（从 lib/tauri.ts 拆分，desktopApi 组合导出不变）。
export const liveIntegrationApi = {
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
