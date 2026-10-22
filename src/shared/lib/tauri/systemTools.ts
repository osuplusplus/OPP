import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type { BeatmapPreviewInspection, BeatmapPreviewRequest, BeatmapPreviewResult, DefaultFileClients, LazerDedupeProgress, LazerDedupeResult, LazerDiskUsage, OsuClient, OtdStatus, PlatformCapabilities, VideoFixProgress, VideoFixResult, VideoScanResult } from "../../types/osu";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { call, isTauri } from "./invoke";

// systemTools 域的 Tauri 命令适配（从 lib/tauri.ts 拆分，desktopApi 组合导出不变）。
export const systemToolsApi = {
  getCapabilities: () => call<PlatformCapabilities>("get_capabilities"),

  getLazerDiskUsage: () => call<LazerDiskUsage>("get_lazer_disk_usage"),

  dedupeLazerFiles: (dryRun: boolean) =>
    call<LazerDedupeResult>("dedupe_lazer_files", { dryRun }),

  cancelLazerDedupe: () => call<void>("cancel_lazer_dedupe"),

  scanOsuVideos: (root?: string | null) =>
    call<VideoScanResult>("scan_osu_videos", { root: root ?? null }),

  fixOsuVideos: (root?: string | null) =>
    call<VideoFixResult>("fix_osu_videos", { root: root ?? null }),

  cancelVideoFix: () => call<void>("cancel_video_fix"),

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

  exitApp: () => call<void>("exit_app"),

  openMediaInExplorer: (client: OsuClient, path: string) =>
    call<void>("open_media_in_explorer", { client, path }),

  openLocalResourceInExplorer: (client: OsuClient, logicalPath: string) =>
    call<void>("open_local_resource_in_explorer", { client, logicalPath }),

  getDefaultFileClients: () => call<DefaultFileClients>("get_default_file_clients"),

  setDefaultFileClient: (kind: "beatmap" | "skin", client: OsuClient) =>
    call<void>("set_default_file_client", { kind, client }),

  setDisplayGamma: (gamma: number) => call<void>("set_display_gamma", { gamma }),

  getOtdStatus: () => call<OtdStatus>("get_otd_status"),

  chooseOtdExecutable: async (current?: string | null) => {
    if (!isTauri()) return null;
    const selected = await openDialog({ defaultPath: current ?? undefined, multiple: false, directory: false, title: "选择 OpenTabletDriver 可执行文件", filters: [{ name: "OpenTabletDriver", extensions: ["exe", "AppImage", "bin"] }] });
    return typeof selected === "string" ? selected : null;
  },

  setOtdExecutable: (path: string) => call<OtdStatus>("set_otd_executable", { path }),

  startOtd: () => call<void>("start_otd"),

  stopOtd: () => call<void>("stop_otd"),

  readOtdConfigSummary: () => call<OtdStatus>("read_otd_config_summary"),

  backupOtdConfig: (destinationDir: string) => call<string>("backup_otd_config", { destinationDir }),

  openOtd: () => call<void>("open_otd"),

  openNeteaseMusicSearch: (artist: string, title: string) =>
    call<void>("open_netease_music_search", { artist, title }),

  onLazerDedupeProgress: async (
    handler: (progress: LazerDedupeProgress) => void,
  ): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<LazerDedupeProgress>("lazer-dedupe-progress", (event) =>
      handler(event.payload),
    );
  },

  onVideoFixProgress: async (
    handler: (progress: VideoFixProgress) => void,
  ): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<VideoFixProgress>("video-fix-progress", (event) =>
      handler(event.payload),
    );
  },
};
