import type { TournamentImportProgress, TournamentLink, TournamentPool, TournamentPoolRef, TournamentPoolSyncResult } from "../../types/osu";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { call, isTauri } from "./invoke";

// tournamentPools 域的 Tauri 命令适配（从 lib/tauri.ts 拆分，desktopApi 组合导出不变）。
export const tournamentPoolsApi = {
  openTournamentPool: (reference: TournamentPoolRef, requestId: number) => call<{ folder_id: string; existing: boolean }>("open_tournament_pool", { reference, requestId }),

  onTournamentImportProgress: async (handler: (progress: TournamentImportProgress) => void): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<TournamentImportProgress>("tournament-pool-import-progress", (event) => handler(event.payload));
  },

  repairTournamentPoolMetadata: (folderId: string) => call<[number, number]>("repair_tournament_pool_metadata", { folderId }),

  getTournamentPool: (reference: TournamentPoolRef) => call<TournamentPool>("get_tournament_pool", { reference }),

  syncTournamentPoolCollection: (reference: TournamentPoolRef) => call<TournamentPoolSyncResult>("sync_tournament_pool_collection", { reference }),

  getPendingTournamentLink: () => call<TournamentLink | null>("get_pending_tournament_link"),

  acknowledgeTournamentLink: (id: number) => call<void>("acknowledge_tournament_link", { id }),

  onTournamentPoolOpen: async (handler: (link: TournamentLink) => void): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<TournamentLink>("tournament-pool-open", (event) => handler(event.payload));
  },
};
