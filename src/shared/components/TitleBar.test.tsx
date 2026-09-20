import { act, render, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { TitleBar } from "./TitleBar";

const windowMock = vi.hoisted(() => ({
  isMaximized: vi.fn(), onResized: vi.fn(), toggleMaximize: vi.fn(),
}));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => windowMock }));
vi.mock("../lib/tauri", () => ({ isTauri: () => true }));

it("tracks native maximize and restore events and removes its listener", async () => {
  let resize = () => {};
  const off = vi.fn();
  windowMock.isMaximized.mockResolvedValue(true);
  windowMock.onResized.mockImplementation(async (callback) => { resize = callback; return off; });
  const { unmount } = render(<TitleBar />);
  expect(await screen.findByRole("button", { name: "还原窗口" })).toBeVisible();
  windowMock.isMaximized.mockResolvedValue(false);
  await act(async () => resize());
  await waitFor(() => expect(screen.getByRole("button", { name: "最大化" })).toBeVisible());
  unmount();
  expect(off).toHaveBeenCalledOnce();
});
