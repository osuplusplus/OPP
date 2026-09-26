import { useEffect } from "react";
import { useQueryClient, type QueryClient } from "@tanstack/react-query";
import { desktopApi } from "../../shared/lib/tauri";
import type { ReplayRenderProgress } from "../../shared/types/osu";

export interface LiveExportProgress { phase: string; frame: number; total: number; message: string }
const subscriptions = new WeakMap<QueryClient, { users: number; dispose: () => void }>();

/** App shell owns these subscriptions so route changes cannot lose completion. */
export function useReplayRenderEvents() {
  const cache = useQueryClient();
  useEffect(() => {
    let entry = subscriptions.get(cache);
    if (!entry) {
      let disposed = false;
      const cleanup: Array<() => void> = [];
      const register = (promise: Promise<() => void>) => {
        void promise.then((off) => { if (disposed) off(); else cleanup.push(off); }).catch(() => undefined);
      };
      register(desktopApi.onReplayRenderProgress((progress) => {
        cache.setQueryData(["replay-studio-session", `ordr-event:${progress.render_id}`], progress);
        cache.setQueryData<ReplayRenderProgress | null>(["replay-studio-session", "ordr-progress"], (current) => current?.render_id === progress.render_id ? progress : current);
      }));
      register(desktopApi.onLiveRenderExport((progress) => {
        cache.setQueryData(["replay-studio-session", "live-export-progress"], progress.phase === "done" ? null : progress);
        if (progress.phase === "done") cache.setQueryData(["replay-studio-session", "live-export-result"], progress.message);
      }));
      entry = { users: 0, dispose: () => { disposed = true; cleanup.forEach((off) => off()); } };
      subscriptions.set(cache, entry);
    }
    entry.users++;
    return () => {
      entry.users--;
      if (entry.users === 0) { entry.dispose(); subscriptions.delete(cache); }
    };
  }, [cache]);
}
