import { musicDesktop } from "../../shared/lib/musicTauri";

export type CollectionTaskPhase =
  | "checking"
  | "downloading"
  | "installing"
  | "writing"
  | "opening"
  | "completed"
  | "failed"
  | "cancelled";

export interface CollectionTaskStatus {
  phase: CollectionTaskPhase;
  message: string;
  processed: number;
  total: number;
  errors: string[];
}

const eventName = "opp:collection-task-status";
let latest: CollectionTaskStatus | null = null;
let cancellationRequested = false;

export function beginCollectionTask(status: CollectionTaskStatus) {
  cancellationRequested = false;
  publishCollectionTask(status);
}

export function requestCollectionTaskCancellation() {
  cancellationRequested = true;
  updateCollectionTask({ phase: "cancelled", message: "正在取消收藏夹同步，当前步骤结束后将停止…" });
}

export function throwIfCollectionTaskCancelled() {
  if (cancellationRequested) {
    throw { code: "COLLECTION_TASK_CANCELLED", message: "收藏夹同步已取消" };
  }
}

export function publishCollectionTask(status: CollectionTaskStatus) {
  void musicDesktop.frontendTask(!["completed", "failed", "cancelled"].includes(status.phase)).catch(() => {});
  latest = status;
  window.dispatchEvent(new CustomEvent<CollectionTaskStatus>(eventName, { detail: status }));
}

export function updateCollectionTask(update: Partial<CollectionTaskStatus>) {
  if (latest?.phase === "cancelled" && update.phase !== "checking") return;
  publishCollectionTask({
    phase: latest?.phase ?? "checking",
    message: latest?.message ?? "正在准备收藏夹同步…",
    processed: latest?.processed ?? 0,
    total: latest?.total ?? 0,
    errors: latest?.errors ?? [],
    ...update,
  });
}

export function subscribeCollectionTask(handler: (status: CollectionTaskStatus) => void) {
  const listener = (event: Event) => handler((event as CustomEvent<CollectionTaskStatus>).detail);
  window.addEventListener(eventName, listener);
  if (latest) handler(latest);
  return () => window.removeEventListener(eventName, listener);
}
