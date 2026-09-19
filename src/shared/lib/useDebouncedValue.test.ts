import { act, renderHook } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { useDebouncedValue } from "./useDebouncedValue";

afterEach(() => vi.useRealTimers());

it("coalesces rapid search changes into the latest value", () => {
  vi.useFakeTimers();
  const { result, rerender, unmount } = renderHook(
    ({ value }) => useDebouncedValue(value), { initialProps: { value: "" } },
  );
  rerender({ value: "a" });
  act(() => vi.advanceTimersByTime(100));
  rerender({ value: "ab" });
  act(() => vi.advanceTimersByTime(199));
  expect(result.current).toBe("");
  act(() => vi.advanceTimersByTime(1));
  expect(result.current).toBe("ab");
  unmount();
});

it("clears pending updates when the search unmounts", () => {
  vi.useFakeTimers();
  const { rerender, unmount } = renderHook(
    ({ value }) => useDebouncedValue(value), { initialProps: { value: "" } },
  );
  rerender({ value: "pending" });
  unmount();
  expect(vi.getTimerCount()).toBe(0);
});
