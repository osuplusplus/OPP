import { useSyncExternalStore } from "react";
import { emptyMusicState, musicDesktop } from "../../shared/lib/musicTauri";
import type { MusicState } from "../../shared/types/music";

let state = emptyMusicState;
const subscribers = new Set<() => void>();
let stop: (() => void) | undefined;
export function acceptMusicState(next: MusicState) {
  if (next.version < state.version) return;
  state = next;
  subscribers.forEach((listener) => listener());
}
function subscribe(listener: () => void) {
  subscribers.add(listener);
  if (subscribers.size === 1) {
    let active = true;
    let dispose: (() => void) | undefined;
    stop = () => { active = false; dispose?.(); };
    void musicDesktop.subscribe((next) => { if (active) acceptMusicState(next); }).then(async (unlisten) => {
      if (!active) { unlisten(); return; }
      dispose = unlisten;
      const next = await musicDesktop.state();
      if (active) acceptMusicState(next);
    }).catch(() => { /* Controls report connection errors without interrupting library browsing. */ });
  }
  return () => { subscribers.delete(listener); if (!subscribers.size) { stop?.(); stop = undefined; } };
}
export function useMusicState() { return useSyncExternalStore(subscribe, () => state, () => emptyMusicState); }
// Library navigation only needs track identity, not every playback-position update.
export function useMusicResourceId() { return useSyncExternalStore(subscribe, () => state.resource_id, () => emptyMusicState.resource_id); }
export const musicApi = musicDesktop;

export function musicError(error: unknown) { return error instanceof Error ? error.message : (error as { message?: string })?.message ?? "播放器操作失败"; }
export function musicTime(seconds: number) {
  const value = Number.isFinite(seconds) ? Math.floor(Math.max(0, seconds)) : 0;
  return `${Math.floor(value / 60)}:${String(value % 60).padStart(2, "0")}`;
}
