import { act, render, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { desktopApi } from "../../shared/lib/tauri";
import { useReplayRenderEvents, type LiveExportProgress } from "./renderEvents";
import type { ReplayRenderProgress } from "../../shared/types/osu";

function Listener() { useReplayRenderEvents(); return null; }

describe("render event lifetime", () => {
  beforeEach(() => vi.restoreAllMocks());

  it("keeps the app listener after the route leaves and filters cloud updates by job", async () => {
    const cache = new QueryClient();
    let cloud!: (value: ReplayRenderProgress) => void;
    let live!: (value: LiveExportProgress) => void;
    const offCloud = vi.fn();
    const offLive = vi.fn();
    vi.spyOn(desktopApi, "onReplayRenderProgress").mockImplementation(async (handler) => { cloud = handler; return offCloud; });
    vi.spyOn(desktopApi, "onLiveRenderExport").mockImplementation(async (handler) => { live = handler; return offLive; });
    const view = render(<QueryClientProvider client={cache}><Listener /><Listener /></QueryClientProvider>);
    await waitFor(() => expect(cloud).toBeDefined());
    expect(desktopApi.onReplayRenderProgress).toHaveBeenCalledTimes(1);
    view.rerender(<QueryClientProvider client={cache}><Listener /></QueryClientProvider>);
    expect(offCloud).not.toHaveBeenCalled();
    const current: ReplayRenderProgress = { render_id: 10, status: "queued", description: "等待", video_url: null };
    cache.setQueryData(["replay-studio-session", "ordr-progress"], current);
    act(() => cloud({ ...current, render_id: 11, status: "completed" }));
    expect(cache.getQueryData(["replay-studio-session", "ordr-progress"])).toEqual(current);
    act(() => live({ phase: "done", frame: 10, total: 10, message: "C:/video.mp4" }));
    expect(cache.getQueryData(["replay-studio-session", "live-export-result"])).toBe("C:/video.mp4");
    expect(cache.getQueryData(["replay-studio-session", "live-export-progress"])).toBeNull();
    view.unmount();
    expect(offCloud).toHaveBeenCalledTimes(1);
    expect(offLive).toHaveBeenCalledTimes(1);
  });

  it("disposes event registrations that resolve after unmount", async () => {
    let resolveCloud!: (dispose: () => void) => void;
    const offCloud = vi.fn();
    vi.spyOn(desktopApi, "onReplayRenderProgress").mockImplementation(() => new Promise((resolve) => { resolveCloud = resolve; }));
    vi.spyOn(desktopApi, "onLiveRenderExport").mockResolvedValue(() => undefined);
    const view = render(<QueryClientProvider client={new QueryClient()}><Listener /></QueryClientProvider>);
    view.unmount();
    await act(async () => resolveCloud(offCloud));
    expect(offCloud).toHaveBeenCalledTimes(1);
  });
});
