import { act, renderHook } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { emptyMusicState } from "../../shared/lib/musicTauri";
import { acceptMusicState, useMusicResourceId } from "./api";

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
