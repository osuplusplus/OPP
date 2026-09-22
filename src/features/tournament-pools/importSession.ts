import { useSyncExternalStore } from "react";
import { desktopApi } from "../../shared/lib/tauri";
import { errorMessage } from "../../shared/lib/format";
import type { TournamentImportPhase, TournamentImportProgress, TournamentLink } from "../../shared/types/osu";

export interface PoolImportState {
  link: TournamentLink;
  phase: TournamentImportPhase | "queued" | "completed" | "failed";
  folderId?: string;
  existing?: boolean;
  error?: string;
}

/** Survives collection route changes; imports are serialized and only the latest pending link is retained. */
export function createPoolImportSession(api: Pick<typeof desktopApi, "openTournamentPool">) {
  let state: PoolImportState | null = null;
  let pending: TournamentLink | null = null;
  let active = false;
  let lastId = 0;
  const listeners = new Set<() => void>();
  const publish = (next: PoolImportState | null) => { state = next; listeners.forEach((listener) => listener()); };
  const drain = async () => {
    if (active) return;
    active = true;
    try {
      while (pending) {
        const link = pending; pending = null;
        if (state?.link.id === link.id) publish({ link, phase: "fetching" });
        try {
          const result = await api.openTournamentPool(link.reference, link.id);
          if (state?.link.id === link.id) publish({ link, phase: "completed", folderId: result.folder_id, existing: result.existing });
        } catch (error) {
          if (state?.link.id === link.id) publish({ link, phase: "failed", error: errorMessage(error) });
        }
      }
    } finally { active = false; }
  };
  return {
    getSnapshot: () => state,
    subscribe: (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; },
    receive(link: TournamentLink) {
      if (link.id <= lastId) return false;
      lastId = link.id; pending = link;
      publish({ link, phase: active ? "queued" : "fetching" }); void drain(); return true;
    },
    progress(progress: TournamentImportProgress) {
      if (state?.link.id === progress.request_id && !["completed", "failed"].includes(state.phase)) publish({ ...state, phase: progress.phase });
    },
    retry() {
      if (state?.phase !== "failed" || active) return;
      pending = state.link; publish({ link: state.link, phase: "fetching" }); void drain();
    },
    dismiss() { if (state && ["completed", "failed"].includes(state.phase)) publish(null); },
  };
}

export const poolImportSession = createPoolImportSession(desktopApi);
export const usePoolImport = () => useSyncExternalStore(poolImportSession.subscribe, poolImportSession.getSnapshot);
