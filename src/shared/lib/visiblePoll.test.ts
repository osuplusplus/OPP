import { afterEach, expect, it, vi } from "vitest";
import { visiblePoll } from "./visiblePoll";

afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); });

it("does not overlap slow requests and stops after disposal", async () => {
  vi.useFakeTimers();
  let finish!: () => void;
  const poll = vi.fn(() => new Promise<void>((resolve) => { finish = resolve; }));
  const stop = visiblePoll(poll, 100);
  await vi.advanceTimersByTimeAsync(1000);
  expect(poll).toHaveBeenCalledTimes(1);
  stop(); finish();
  await vi.advanceTimersByTimeAsync(1000);
  expect(poll).toHaveBeenCalledTimes(1);
});

it("pauses while hidden and refreshes immediately when visible", async () => {
  vi.useFakeTimers();
  const visibility = vi.spyOn(document, "visibilityState", "get").mockReturnValue("hidden");
  const poll = vi.fn(async () => {});
  const stop = visiblePoll(poll, 100);
  await vi.advanceTimersByTimeAsync(1000);
  expect(poll).not.toHaveBeenCalled();
  visibility.mockReturnValue("visible");
  document.dispatchEvent(new Event("visibilitychange"));
  await vi.advanceTimersByTimeAsync(0);
  expect(poll).toHaveBeenCalledTimes(1);
  visibility.mockReturnValue("hidden");
  document.dispatchEvent(new Event("visibilitychange"));
  await vi.advanceTimersByTimeAsync(1000);
  expect(poll).toHaveBeenCalledTimes(1);
  stop();
});
