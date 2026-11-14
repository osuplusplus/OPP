import type { BeatmapHubAuthStatus, BeatmapHubBackup, BeatmapHubBackupRestoreResult, BeatmapHubBackupUploadResult, BeatmapHubComment, BeatmapHubImportResult, BeatmapHubPack, BeatmapHubPackPreview, BeatmapHubProfile, BeatmapHubPublishResult, BeatmapHubRecommendation, OnlineBeatmapset, OsuCollectorCollectionDetail, OsuCollectorImportProgress, OsuCollectorListPage } from "../../types/osu";
import { listen } from "@tauri-apps/api/event";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { call, isTauri } from "./invoke";

// beatmaphub 域的 Tauri 命令适配（从 lib/tauri.ts 拆分，desktopApi 组合导出不变）。
export const beatmaphubApi = {
  getBeatmapHubAuthStatus: () => call<BeatmapHubAuthStatus>("get_beatmaphub_auth_status"),

  bootstrapBeatmapHub: () => call<BeatmapHubAuthStatus>("bootstrap_beatmaphub"),

  reconnectBeatmapHub: () => call<BeatmapHubAuthStatus>("reconnect_beatmaphub"),

  loginBeatmapHub: () => call<BeatmapHubAuthStatus>("login_beatmaphub"),

  logoutBeatmapHub: () => call<void>("logout_beatmaphub"),

  getBeatmapHubProfile: () => call<BeatmapHubProfile>("get_beatmaphub_profile"),

  revokeBeatmapHubDevice: (deviceId: string) => call<void>("revoke_beatmaphub_device", { deviceId }),

  getBeatmapHubPack: (shareId: string) => call<BeatmapHubPack>("get_beatmaphub_pack", { shareId }),

  getBeatmapHubRecommendations: (limit = 20) => call<BeatmapHubRecommendation[]>("get_beatmaphub_recommendations", { limit }),

  searchBeatmapHubPacks: (query: string, limit = 20) => call<BeatmapHubRecommendation[]>("search_beatmaphub_packs", { query, limit }),

  previewBeatmapHubPack: (shareId: string) => call<BeatmapHubPackPreview>("preview_beatmaphub_pack", { shareId }),

  publishBeatmapHubPack: (folderId: string, title: string, description: string, isPrivate = false) =>
    call<BeatmapHubPublishResult>("publish_beatmaphub_pack", { folderId, title, description, isPrivate }),

  updateBeatmapHubPack: (shareId: string, folderId: string, title: string, description: string, isPrivate = false) =>
    call<void>("update_beatmaphub_pack", { shareId, folderId, title, description, isPrivate }),

  deleteBeatmapHubPack: (shareId: string) => call<void>("delete_beatmaphub_pack", { shareId }),

  rateBeatmapHubPack: (shareId: string, score: number) => call<void>("rate_beatmaphub_pack", { shareId, score }),

  favoriteBeatmapHubPack: (shareId: string, enabled: boolean) => call<void>("favorite_beatmaphub_pack", { shareId, enabled }),

  likeBeatmapHubPack: (shareId: string, enabled: boolean) => call<void>("like_beatmaphub_pack", { shareId, enabled }),

  getBeatmapHubComments: (shareId: string, limit = 50) => call<BeatmapHubComment[]>("get_beatmaphub_comments", { shareId, limit }),

  createBeatmapHubComment: (shareId: string, content: string) => call<BeatmapHubComment>("create_beatmaphub_comment", { shareId, content }),

  updateBeatmapHubComment: (commentId: string, content: string) => call<BeatmapHubComment>("update_beatmaphub_comment", { commentId, content }),

  deleteBeatmapHubComment: (commentId: string) => call<void>("delete_beatmaphub_comment", { commentId }),

  importBeatmapHubPack: (shareId: string, resolved: OnlineBeatmapset[]) =>
    call<BeatmapHubImportResult>("import_beatmaphub_pack", { shareId, resolved }),

  uploadBeatmapBackup: () => call<BeatmapHubBackupUploadResult>("upload_beatmap_backup"),

  getBeatmapBackup: () => call<BeatmapHubBackup>("get_beatmap_backup"),

  deleteBeatmapBackup: () => call<void>("delete_beatmap_backup"),

  restoreBeatmapBackup: () => call<BeatmapHubBackupRestoreResult>("restore_beatmap_backup"),

  // —— osu!Collector 补充来源（直连，只读浏览 + 导入）——
  getOsuCollectorCollections: (params: { search?: string | null; sortBy?: string | null; orderBy?: string | null; cursor?: number | null; perPage?: number | null } = {}) =>
    call<OsuCollectorListPage>("get_osu_collector_collections", {
      search: params.search ?? null,
      sortBy: params.sortBy ?? null,
      orderBy: params.orderBy ?? null,
      cursor: params.cursor ?? null,
      perPage: params.perPage ?? null,
    }),

  getOsuCollectorCollection: (collectionId: number) =>
    call<OsuCollectorCollectionDetail>("get_osu_collector_collection", { collectionId }),

  importOsuCollectorCollection: (collectionId: number) =>
    call<BeatmapHubImportResult>("import_osu_collector_collection", { collectionId }),

  onOsuCollectorImportProgress: async (
    handler: (progress: OsuCollectorImportProgress) => void,
  ): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<OsuCollectorImportProgress>(
      "collector-import-progress",
      (event) => handler(event.payload),
    );
  },
};
