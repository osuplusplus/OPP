import { useSyncExternalStore } from "react";
import { desktopApi } from "../../shared/lib/tauri";
import { errorMessage } from "../../shared/lib/format";
import type { BeatmapDownloadProgress, BeatmapDownloadRequest, BeatmapDownloadResult, OnlineBeatmapset } from "../../shared/types/osu";

export const beatmapDownloadDirectoryKey = "opp:beatmap-download-directory";
type Snapshot = {
  queue: OnlineBeatmapset[];
  activeIds: number[];
  busy: boolean;
  progress: BeatmapDownloadProgress | null;
  result: BeatmapDownloadResult | null;
  error: string | null;
};
type DownloadApi = Pick<typeof desktopApi, "downloadOnlineBeatmapsets" | "onBeatmapDownloadProgress" | "cancelOnlineBeatmapDownload">;

/** App-session state: neither route changes nor closing the drawer owns a download. */
export function createDownloadSession(api: DownloadApi) {
  let state: Snapshot = { queue: [], activeIds: [], busy: false, progress: null, result: null, error: null };
  const listeners = new Set<() => void>();
  let cancellationRequested = false;
  let invoked = false;
  const update = (patch: Partial<Snapshot>) => { state = { ...state, ...patch }; listeners.forEach((listener) => listener()); };
  return {
    getSnapshot: () => state,
    subscribe: (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; },
    add(items: OnlineBeatmapset[]) {
      const queue = new Map(state.queue.map((item) => [item.id, item]));
      let added = 0; let blocked = 0;
      for (const item of items) {
        if (item.availability?.download_disabled) { blocked++; continue; }
        if (!queue.has(item.id) && !state.activeIds.includes(item.id)) { queue.set(item.id, item); added++; }
      }
      update({ queue: [...queue.values()] });
      return { added, blocked, duplicates: items.length - added - blocked };
    },
    remove(id: number) { if (!state.activeIds.includes(id)) update({ queue: state.queue.filter((item) => item.id !== id) }); },
    clear() { update({ queue: state.queue.filter((item) => state.activeIds.includes(item.id)) }); },
    async cancel() {
      if (!state.busy) return;
      cancellationRequested = true;
      if (!invoked) return;
      try { await api.cancelOnlineBeatmapDownload(); }
      catch (error) { update({ error: errorMessage(error) }); }
    },
    async start(items: OnlineBeatmapset[], prepare: () => Promise<Omit<BeatmapDownloadRequest, "items"> | null>) {
      if (state.busy) return;
      const batch = [...new Map(items.filter((item) => !item.availability?.download_disabled).map((item) => [item.id, item])).values()];
      if (!batch.length) return;
      const ids = new Set(batch.map((item) => item.id));
      const succeeded = new Set<number>();
      let dispose: (() => void) | undefined;
      cancellationRequested = false; invoked = false;
      update({ busy: true, activeIds: [...ids], progress: null, result: null, error: null });
      try {
        const options = await prepare();
        if (!options || cancellationRequested) return;
        // Subscribe before invoking, so even very fast skipped-file events are captured.
        dispose = await api.onBeatmapDownloadProgress((progress) => {
          if (progress.current_beatmapset_id !== null && !ids.has(progress.current_beatmapset_id)) return;
          if ((progress.phase === "completed" || progress.phase === "skipped") && progress.current_beatmapset_id !== null) succeeded.add(progress.current_beatmapset_id);
          update({ progress });
        });
        if (cancellationRequested) return;
        invoked = true;
        const result = await api.downloadOnlineBeatmapsets({ ...options, items: batch.map((item) => ({ beatmapset_id: item.id, artist: item.artist, title: item.title })) });
        // The existing backend processes the request in order, stopping at cancellation.
        // Reconcile the processed prefix too, in case a progress event was lost.
        const failed = new Set(result.failures.map((item) => item.beatmapset_id));
        batch.slice(0, result.completed + result.skipped + result.failed).forEach((item) => { if (!failed.has(item.id)) succeeded.add(item.id); });
        update({ result });
      } catch (error) {
        update({ error: errorMessage(error) });
      } finally {
        dispose?.();
        invoked = false;
        update({ queue: state.queue.filter((item) => !succeeded.has(item.id)), busy: false, activeIds: [] });
      }
    },
  };
}

export const downloadSession = createDownloadSession(desktopApi);
export const useDownloadSession = () => useSyncExternalStore(downloadSession.subscribe, downloadSession.getSnapshot);
