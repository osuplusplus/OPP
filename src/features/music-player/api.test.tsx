import { act, renderHook } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { emptyMusicState } from "../../shared/lib/musicTauri";
import { acceptMusicState, useMusicResourceId, useMusicControls } from "./api";

vi.mock("../../shared/lib/musicTauri", async (importOriginal) => {
  const original = await importOriginal<typeof import("../../shared/lib/musicTauri")>();
  return { ...original, musicDesktop: {
    subscribe: vi.fn(async () => () => {}),
    state: vi.fn(async () => original.emptyMusicState),
  } };
});

it("does not rerender library navigation for playback-position ticks", async () => {
  let renders = 0;
  const { result } = renderHook(() => { renders++; return useMusicResourceId(); });
  await act(async () => {});
  act(() => acceptMusicState({ ...emptyMusicState, version: 1, resource_id: "track-a" }));
  const beforeTicks = renders;
  for (let version = 2; version < 20; version++) {
    act(() => acceptMusicState({ ...emptyMusicState, version, resource_id: "track-a", position: version }));
  }
  expect(renders).toBe(beforeTicks);
  expect(result.current).toBe("track-a");
  act(() => acceptMusicState({ ...emptyMusicState, version: 20, resource_id: "track-b" }));
  expect(result.current).toBe("track-b");
  expect(renders).toBe(beforeTicks + 1);
});

it("keeps controls stable when only playback position changes", async () => {
  let renders = 0;
  const { result } = renderHook(() => { renders++; return useMusicControls(); });
  await act(async () => {});
  act(() => acceptMusicState({ ...emptyMusicState, version: 100, playing: true }));
  const before = renders;
  act(() => acceptMusicState({ ...emptyMusicState, version: 101, playing: true, position: 42 }));
  expect(renders).toBe(before);
  expect(result.current.playing).toBe(true);
  act(() => acceptMusicState({ ...emptyMusicState, version: 102, playing: false, position: 42 }));
  expect(renders).toBe(before + 1);
});

it("retains hidden playback updates and publishes once on visibility restoration", async () => {
  const visibility = vi.spyOn(document, "visibilityState", "get").mockReturnValue("visible");
  let renders = 0;
  const { result, unmount } = renderHook(() => { renders++; return useMusicResourceId(); });
  await act(async () => {});
  const before = renders;
  visibility.mockReturnValue("hidden");
  act(() => acceptMusicState({ ...emptyMusicState, version: 200, resource_id: "hidden-track" }));
  expect(renders).toBe(before);
  visibility.mockReturnValue("visible");
  act(() => document.dispatchEvent(new Event("visibilitychange")));
  expect(result.current).toBe("hidden-track");
  unmount(); visibility.mockRestore();
});
