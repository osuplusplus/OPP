import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { desktopApi } from "../shared/lib/tauri";
import type { GameStatusSnapshot } from "../shared/types/osu";
import { GameLauncher } from "./GameLauncher";
import { ModeProvider } from "./ModeContext";

beforeEach(() => {
  localStorage.clear();
  vi.spyOn(desktopApi, "getGameStatus").mockResolvedValue({ clients: [] });
  vi.spyOn(desktopApi, "onGameStatusChanged").mockResolvedValue(() => undefined);
  vi.spyOn(desktopApi, "startGameSession").mockResolvedValue(undefined as never);
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

const mount = () => render(<ModeProvider><GameLauncher /></ModeProvider>);
const runningStatus: GameStatusSnapshot = { clients: [{ client: "lazer", running: true, executable: null, detected_at: "2026-09-14" }] };

describe("GameLauncher", () => {
  it("launches the saved client and mode directly, and can launch the other client", async () => {
    localStorage.setItem("opp.global-client", "lazer");
    localStorage.setItem("opp.global-ruleset", "mania");
    const user = userEvent.setup();
    mount();
    await screen.findByText("游戏未运行");
    await user.click(screen.getByRole("button", { name: /启动 osu!/ }));
    expect(desktopApi.startGameSession).toHaveBeenCalledWith("mania", "lazer");
    await user.click(screen.getByRole("button", { name: "选择其他客户端启动" }));
    await user.click(screen.getByRole("button", { name: "启动 osu! Stable" }));
    expect(desktopApi.startGameSession).toHaveBeenLastCalledWith("mania", "stable");
    expect(localStorage.getItem("opp.global-client")).toBe("lazer");
  });

  it("updates running status from events and prevents duplicate launches", async () => {
    localStorage.setItem("opp.global-client", "lazer");
    let update!: (status: GameStatusSnapshot) => void;
    vi.mocked(desktopApi.onGameStatusChanged).mockImplementation(async (handler) => { update = handler; return () => undefined; });
    mount();
    await screen.findByText("游戏未运行");
    act(() => update(runningStatus));
    expect(screen.getByText("Lazer 运行中")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /osu! 运行中/ })).toBeDisabled();
    act(() => update({ clients: [] }));
    expect(screen.getByRole("button", { name: /启动 osu!/ })).toBeEnabled();
  });

  it("keeps live status when an older initial request finishes later", async () => {
    let resolve!: (status: GameStatusSnapshot) => void;
    let update!: (status: GameStatusSnapshot) => void;
    vi.mocked(desktopApi.getGameStatus).mockReturnValue(new Promise((done) => { resolve = done; }));
    vi.mocked(desktopApi.onGameStatusChanged).mockImplementation(async (handler) => { update = handler; return () => undefined; });
    mount();
    act(() => update(runningStatus));
    await act(async () => resolve({ clients: [] }));
    expect(screen.getByText("Lazer 运行中")).toBeInTheDocument();
  });

  it("shows launch failures and permits retry", async () => {
    vi.mocked(desktopApi.startGameSession).mockRejectedValue({ message: "未找到游戏目录" });
    const user = userEvent.setup();
    mount();
    await user.click(screen.getByRole("button", { name: /启动 osu!/ }));
    expect(await screen.findByRole("alert")).toHaveTextContent("未找到游戏目录");
    expect(screen.getByRole("button", { name: /启动 osu!/ })).toBeEnabled();
  });

  it("cleans up subscriptions that resolve after unmount", async () => {
    let resolve!: (off: () => void) => void;
    const off = vi.fn();
    vi.mocked(desktopApi.onGameStatusChanged).mockReturnValue(new Promise((done) => { resolve = done; }));
    const view = mount();
    view.unmount();
    await act(async () => resolve(off));
    await waitFor(() => expect(off).toHaveBeenCalledOnce());
  });
});
