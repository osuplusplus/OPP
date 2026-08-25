import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ModeProvider } from "../../app/ModeContext";
import { desktopApi } from "../../shared/lib/tauri";
import { ReplayRenderPage } from "./ReplayRenderPage";

function renderPage() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={queryClient}><ModeProvider><MemoryRouter><ReplayRenderPage /></MemoryRouter></ModeProvider></QueryClientProvider>);
}

describe("ReplayRenderPage", () => {
  beforeEach(() => {
    window.localStorage.clear();
    vi.spyOn(desktopApi, "getDanserStatus").mockResolvedValue({ available: true, executable_path: "C:\\danser\\danser-cli.exe", ffmpeg_available: true, profiles: ["default"], message: "ready" });
    vi.spyOn(desktopApi, "listGameMedia").mockResolvedValue([]);
    vi.spyOn(desktopApi, "getDanserRenderQueue").mockResolvedValue([]);
  });

  it("switches between the local and online subpages and remembers the choice", async () => {
    const user = userEvent.setup();
    renderPage();
    expect(screen.getByRole("tab", { name: "实时预览" })).toHaveAttribute("aria-selected", "true");
    await user.click(screen.getByRole("tab", { name: "本地 Danser" }));
    expect(screen.getByRole("tab", { name: "本地 Danser" })).toHaveAttribute("aria-selected", "true");
    expect(await screen.findByText("本地运行环境")).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "在线 o!rdr" }));
    expect(screen.getByRole("tab", { name: "在线 o!rdr" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByText(/o!rdr 只接收回放文件/)).toBeInTheDocument();
    expect(window.localStorage.getItem("opp:replay-render-provider")).toBe("ordr");
  });

  it("shows the supported Danser version requirement", async () => {
    renderPage();
    expect(await screen.findByText("版本要求：Danser 0.11.x")).toBeInTheDocument();
  });

  it("shows the export success panel when the backend reports the done phase", async () => {
    const user = userEvent.setup();
    let onProgress: ((progress: { phase: string; frame: number; total: number; message: string }) => void) | undefined;
    vi.spyOn(desktopApi, "onLiveRenderExport").mockImplementation(async (handler) => {
      onProgress = handler;
      return () => undefined;
    });
    vi.spyOn(desktopApi, "listGameMedia").mockResolvedValue([
      { client: "stable", path: "/osr/replay.osr", kind: "replay", modified_at: null, size: 1 },
    ]);
    vi.spyOn(desktopApi, "inspectGameReplay").mockResolvedValue({
      path: "/osr/replay.osr", beatmap_hash: "hash", username: "player", beatmap_id: 1,
      beatmap_resource_id: "123", beatmap_title: "Song", submitted: true,
    });
    vi.spyOn(desktopApi, "liveRenderCheckFfmpeg").mockResolvedValue("ffmpeg version 7");
    vi.spyOn(desktopApi, "liveRenderCheckNvenc").mockResolvedValue([true, true]);
    renderPage();

    const exportButton = await screen.findByRole("button", { name: "导出视频" });
    await waitFor(() => expect(exportButton).toBeEnabled());
    await user.click(exportButton);
    onProgress!({ phase: "render", frame: 1, total: 10, message: "1/10" });
    expect(await screen.findByText("正在渲染 1/10 帧")).toBeInTheDocument();

    onProgress!({ phase: "mux", frame: 10, total: 10, message: "混入音频…" });
    expect(await screen.findByText("混入音频…")).toBeInTheDocument();

    onProgress!({ phase: "done", frame: 10, total: 10, message: "/videos/replay.mp4" });
    expect(await screen.findByText(/导出完成/)).toBeInTheDocument();
    expect(screen.getByText(/\/videos\/replay\.mp4/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "打开所在文件夹" })).toBeInTheDocument();
    expect(screen.queryByText(/正在渲染/)).not.toBeInTheDocument();
  });
});
