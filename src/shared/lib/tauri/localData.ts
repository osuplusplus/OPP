import type { CareerCalendar, CareerCaptureResult, CareerDayDetail, CareerStatus, LocalBeatmapPresence, LocalDatabaseStatus, LocalLibraryStorageStatus, OsuClient, Ruleset } from "../../types/osu";
import { call } from "./invoke";

// localData 域的 Tauri 命令适配（从 lib/tauri.ts 拆分，desktopApi 组合导出不变）。
export const localDataApi = {
  getLocalDatabaseStatus: () => call<LocalDatabaseStatus>("get_local_database_status"),

  getLocalLibraryStorageStatus: () => call<LocalLibraryStorageStatus[]>("get_local_library_storage_status"),

  getCareerStatus: () => call<CareerStatus>("get_career_status"),

  captureCareerSnapshot: (ruleset: Ruleset, forceRefresh = false) => call<CareerCaptureResult>("capture_career_snapshot", { ruleset, forceRefresh }),

  getCareerCalendar: (ruleset: Ruleset, startDate: string, endDate: string) => call<CareerCalendar>("get_career_calendar", { ruleset, startDate, endDate }),

  getCareerDay: (ruleset: Ruleset, date: string) => call<CareerDayDetail>("get_career_day", { ruleset, date }),

  clearCareerHistory: (beforeDate?: string) => call<void>("clear_career_history", { beforeDate }),

  migrateLocalLibraryDatabase: () => call<LocalLibraryStorageStatus[]>("migrate_local_library_database"),

  queryLocalBeatmapPresence: (ids: number[], client: OsuClient | null) => call<LocalBeatmapPresence[]>("query_local_beatmap_presence", { ids, client }),

  initializeLocalDatabase: (directory: string) => call<LocalDatabaseStatus>("initialize_local_database", { directory }),

  retryLocalDatabase: () => call<LocalDatabaseStatus>("retry_local_database"),

  locateLocalDatabase: (directory: string) => call<LocalDatabaseStatus>("locate_local_database", { directory }),
};
