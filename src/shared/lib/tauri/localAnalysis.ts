import type { BeatmapQuery, LocalArtwork, LocalBeatmapAudioPayload, LocalBeatmapDetail, LocalBeatmapSetSummary, LocalBeatmapSummary, LocalIndexLoadStatus, LocalLibrarySummary, LocalScanProgress, LocalSkinAssetPayload, LocalSkinDetail, LocalSkinPreview, LocalSkinSummary, LocalSourceStatus, OsuClient, Page, Ruleset, SkinQuery } from "../../types/osu";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { call, isTauri } from "./invoke";

// localAnalysis 域的 Tauri 命令适配（从 lib/tauri.ts 拆分，desktopApi 组合导出不变）。
export const localAnalysisApi = {
  getLocalSources: () =>
    call<LocalSourceStatus[]>("get_local_sources"),

  getLocalIndexStatus: () =>
    call<LocalIndexLoadStatus>("get_local_index_status"),

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

  getLocalBeatmapSet: (client: OsuClient, setKey: string, ruleset: Ruleset) =>
    call<LocalBeatmapSetSummary>("get_local_beatmap_set", { client, setKey, ruleset }),

  pickRandomLocalBeatmapSet: (query: BeatmapQuery, excludeSetKey: string | null = null) =>
    call<LocalBeatmapSetSummary | null>("pick_random_local_beatmap_set", { query, excludeSetKey }),

  getLocalBeatmapAudio: (client: OsuClient, resourceId: string) =>
    call<LocalBeatmapAudioPayload>("get_local_beatmap_audio", { client, resourceId }),

  getLocalBeatmapDetail: (client: OsuClient, resourceId: string) =>
    call<LocalBeatmapDetail>("get_local_beatmap_detail", {
      client,
      resourceId,
    }),

  getLocalBeatmapPath: (client: OsuClient, resourceId: string) =>
    call<string>("get_local_beatmap_path", { client, resourceId }),

  getLocalArtworkSample: () => call<LocalArtwork[]>("get_local_artwork_sample"),

  getLocalBeatmapBackground: (client: OsuClient, resourceId: string, size: "thumbnail" | "stage" = "thumbnail") =>
    call<string | null>("get_local_beatmap_background", {
      client,
      resourceId,
      size,
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

  onLocalScanProgress: async (
    handler: (progress: LocalScanProgress) => void,
  ): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<LocalScanProgress>("local-scan-progress", (event) =>
      handler(event.payload),
    );
  },
};
