import type { CollectionArtwork, CollectionBackupStatus, CollectionBrowsePage, CollectionBrowseQuery, CollectionCandidate, CollectionDownloadItem, CollectionEntryPage, CollectionFolder, CollectionInstallResult, CollectionManagerStatus, CollectionOpenResult, CollectionPersonalRecord, CollectionSharePreview, CollectionSnapshot, CollectionSummaries, CollectionSyncStatus, CollectionTaskProgress, CollectionWriteResult, LocalScoreRecord, LocalScoreStatus, OsuClient } from "../../types/osu";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { call, isTauri } from "./invoke";

// collections 域的 Tauri 命令适配（从 lib/tauri.ts 拆分，desktopApi 组合导出不变）。
export const collectionsApi = {
  openLazerBeatmap: (beatmapId: number) => call<void>("open_lazer_beatmap", { beatmapId }),

  enableCollectionStableSync: (folderId: string) => call<void>("enable_collection_stable_sync", { folderId }),

  refreshCollectionSummaries: (client: OsuClient) => call<CollectionSummaries>("refresh_collection_summaries", { client }),

  listCollectionSummaries: () => call<CollectionSummaries>("list_collection_summaries"),

  queryCollectionBrowser: (query: CollectionBrowseQuery) => call<CollectionBrowsePage>("query_collection_browser", { query }),

  saveCollectionRecord: (folderId: string, entryId: string, record: CollectionPersonalRecord) => call<CollectionPersonalRecord>("save_collection_record", { folderId, entryId, record }),

  getCollectionRecord: (folderId: string, entryId: string) => call<CollectionPersonalRecord>("get_collection_record", { folderId, entryId }),

  getCollectionEntryScores: (folderId: string, entryId: string, player: string | null) => call<LocalScoreRecord[]>("get_collection_entry_scores", { folderId, entryId, player }),

  getCollectionArtwork: (folderId: string | null, offset = 0) => call<CollectionArtwork[]>("get_collection_artwork", { folderId, offset }),

  refreshLocalScores: (force = false) => call<LocalScoreStatus>("refresh_local_scores", { force }),

  queryCollectionEntries: (folderId: string, offset: number, limit: number, revision: number) => call<CollectionEntryPage>("query_collection_entries", { folderId, offset, limit, revision }),

  listCollections: () => call<CollectionSnapshot>("list_collections"),

  getCollectionSyncStatus: () => call<CollectionSyncStatus>("get_collection_sync_status"),

  refreshCollections: (client: OsuClient) => call<CollectionSnapshot>("refresh_collections", { client }),

  createCollection: (name: string, creator: string) => call<CollectionFolder>("create_collection", { name, creator }),

  renameCollection: (folderId: string, name: string) => call<void>("rename_collection", { folderId, name }),

  deleteCollection: (folderId: string) => call<void>("delete_collection", { folderId }),

  addCollectionEntries: (folderId: string, candidates: CollectionCandidate[]) => call<void>("add_collection_entries", { folderId, candidates }),

  removeCollectionEntry: (folderId: string, entryId: string) => call<void>("remove_collection_entry", { folderId, entryId }),

  writeStableCollections: () => call<CollectionWriteResult>("write_stable_collections"),

  getCollectionManagerStatus: () => call<CollectionManagerStatus>("get_collection_manager_status"),

  setCollectionManagerPath: (path: string | null) => call<void>("set_collection_manager_path", { path }),

  getCollectionBackupStatus: (client: OsuClient) => call<CollectionBackupStatus>("get_collection_backup_status", { client }),

  createCollectionBackup: (client: OsuClient) => call<CollectionBackupStatus>("create_collection_backup", { client }),

  writeLazerCollections: () => call<CollectionWriteResult>("write_lazer_collections"),

  restoreCollectionBackup: (client: OsuClient, backupPath: string) => call<void>("restore_collection_backup", { client, backupPath }),

  exportCollectionShare: (folderId: string, creator: string) => call<string>("export_collection_share", { folderId, creator }),

  previewCollectionShare: (code: string) => call<CollectionSharePreview>("preview_collection_share", { code }),

  importCollectionShare: (code: string) => call<CollectionFolder>("import_collection_share", { code }),

  importCollectionArchive: (path: string) => call<CollectionFolder>("import_collection_archive", { path }),

  getCollectionDownloadItems: (folderIds: string[]) => call<CollectionDownloadItem[]>("get_collection_download_items", { folderIds }),

  beginCollectionTask: () => call<void>("begin_collection_task"),

  cancelCollectionTask: () => call<void>("cancel_collection_task"),

  installCollectionDownloads: (folderIds: string[], archivePaths: string[]) =>
    call<CollectionInstallResult>("install_collection_downloads", { folderIds, archivePaths }),

  openCollectionDownloads: (archivePaths: string[]) =>
    call<CollectionOpenResult>("open_collection_downloads", { archivePaths }),

  onCollectionTaskProgress: async (
    handler: (progress: CollectionTaskProgress) => void,
  ): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<CollectionTaskProgress>("collection-task-progress", (event) =>
      handler(event.payload),
    );
  },
};
