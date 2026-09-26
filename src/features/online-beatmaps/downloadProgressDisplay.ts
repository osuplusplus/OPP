import type { BeatmapDownloadProgress } from "../../shared/types/osu";

const terminalPhases = new Set<BeatmapDownloadProgress["phase"]>([
  "completed",
  "skipped",
  "failed",
]);

/**
 * Keeps one item in view while a batch has multiple downloads in flight.
 *
 * The downloader emits an event for whichever worker reported most recently.
 * Showing that event directly makes the title (and the per-file byte count)
 * jump between workers. This small stateful adapter keeps the first active
 * item visible until it finishes, while still using the latest aggregate
 * counters from every event.
 */
export function createDownloadProgressDisplay() {
  let activeId: number | null = null;
  let order: number[] = [];
  const snapshots = new Map<number, BeatmapDownloadProgress>();
  let maxProcessed = 0;
  let maxCompleted = 0;
  let maxSkipped = 0;
  let maxFailed = 0;

  const reset = () => {
    activeId = null;
    order = [];
    snapshots.clear();
    maxProcessed = 0;
    maxCompleted = 0;
    maxSkipped = 0;
    maxFailed = 0;
  };

  const update = (progress: BeatmapDownloadProgress | null) => {
    if (!progress) {
      reset();
      return null;
    }

    if (progress.phase === "started") {
      reset();
      return progress;
    }
    if (progress.phase === "finished" || progress.phase === "cancelled") {
      const result = progress;
      reset();
      return result;
    }

    maxProcessed = Math.max(maxProcessed, progress.processed);
    maxCompleted = Math.max(maxCompleted, progress.completed);
    maxSkipped = Math.max(maxSkipped, progress.skipped);
    maxFailed = Math.max(maxFailed, progress.failed);
    const normalized = {
      ...progress,
      processed: maxProcessed,
      completed: maxCompleted,
      skipped: maxSkipped,
      failed: maxFailed,
    };

    const id = normalized.current_beatmapset_id;
    if (id === null) return activeId === null ? normalized : mergeSnapshot(normalized, snapshots.get(activeId));

    if (!snapshots.has(id)) order = [...order, id];
    snapshots.set(id, normalized);

    const current = activeId === null ? null : snapshots.get(activeId);
    if (activeId === null || (current && terminalPhases.has(current.phase) && !terminalPhases.has(normalized.phase))) {
      activeId = id;
    }

    if (terminalPhases.has(normalized.phase) && activeId === id) {
      const next = order.find((candidate) => {
        const snapshot = snapshots.get(candidate);
        return snapshot && !terminalPhases.has(snapshot.phase);
      });
      if (next !== undefined) activeId = next;
    }

    return activeId === id ? normalized : mergeSnapshot(normalized, snapshots.get(activeId ?? id));
  };

  return { reset, update };
}

function mergeSnapshot(
  latest: BeatmapDownloadProgress,
  snapshot: BeatmapDownloadProgress | undefined,
) {
  if (!snapshot) return latest;
  return {
    ...latest,
    current_beatmapset_id: snapshot.current_beatmapset_id,
    current_title: snapshot.current_title ?? latest.current_title,
    downloaded_bytes: snapshot.downloaded_bytes,
    total_bytes: snapshot.total_bytes,
    bytes_per_second: snapshot.bytes_per_second,
  };
}
