import type { BeatmapCalculationRequest, BeatmapCalculationResult, BeatmapDownloadProgress, BeatmapDownloadRequest, BeatmapDownloadResult, BeatmapOpenResult, BeatmapSourceStatus, CollectedBeatmapsets, OnlineBeatmapSearchQuery, OnlineBeatmapSearchResponse, OnlineBeatmapset, OsuClient } from "../../types/osu";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { call, isTauri } from "./invoke";

// onlineBeatmaps 域的 Tauri 命令适配（从 lib/tauri.ts 拆分，desktopApi 组合导出不变）。
export const onlineBeatmapsApi = {
  searchOnlineBeatmapsets: (query: OnlineBeatmapSearchQuery) =>
    call<OnlineBeatmapSearchResponse>("search_online_beatmapsets", { query }),

  collectOnlineBeatmapsets: (
    query: OnlineBeatmapSearchQuery,
    limit: number,
  ) =>
    call<CollectedBeatmapsets>("collect_online_beatmapsets", { query, limit }),

  getOnlineBeatmapset: (beatmapsetId: number) =>
    call<OnlineBeatmapset>("get_online_beatmapset", { beatmapsetId }),

  getOnlineBeatmapBackground: (beatmapsetId: number) =>
    call<string | null>("get_online_beatmap_background", { beatmapsetId }),

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

  openDownloadedPath: (path: string) => call<void>("open_downloaded_path", { path }),

  openBeatmapFiles: (client: OsuClient, archivePaths: string[]) =>
    call<BeatmapOpenResult>("open_beatmap_files", { client, archivePaths }),

  onBeatmapDownloadProgress: async (
    handler: (progress: BeatmapDownloadProgress) => void,
  ): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<BeatmapDownloadProgress>(
      "beatmap-download-progress",
      (event) => handler(event.payload),
    );
  },
};
