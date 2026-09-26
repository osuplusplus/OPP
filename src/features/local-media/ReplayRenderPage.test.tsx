import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ModeProvider } from "../../app/ModeContext";
import { desktopApi } from "../../shared/lib/tauri";
import type { GameMediaItem, ReplayMapInfo } from "../../shared/types/osu";
import { ReplayRenderPage } from "./ReplayRenderPage";

function renderPage(queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } }), route = "/local/media/render") {
  return render(<QueryClientProvider client={queryClient}><ModeProvider><MemoryRouter initialEntries={[route]}><ReplayRenderPage /></MemoryRouter></ModeProvider></QueryClientProvider>);
}

const external: GameMediaItem = { client: "stable", path: "C:/Desktop/精彩 回放.osr", kind: "replay", modified_at: null, size: 100 };
const mapInfo = (path: string): ReplayMapInfo => ({ path, ruleset: "osu", beatmap_hash: "hash", username: "player", beatmap_id: 1, beatmap_resource_id: "123", beatmap_title: `Song ${path}`, submitted: true });

describe("ReplayRenderPage", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.stubGlobal("ResizeObserver", class { observe() {} disconnect() {} });
    window.localStorage.clear();
    vi.spyOn(desktopApi, "getDanserStatus").mockResolvedValue({ available: true, executable_path: "C:\\danser\\danser-cli.exe", ffmpeg_available: true, profiles: ["default"], message: "ready" });
    vi.spyOn(desktopApi, "listGameMedia").mockResolvedValue([]);
    vi.spyOn(desktopApi, "getDanserRenderQueue").mockResolvedValue([]);
  });

  it("defaults to live preview, switches providers, and remembers the choice", async () => {
    const user = userEvent.setup();
    renderPage();
    // 新用户默认进入实时预览，仍可切换到本地与在线渲染。
    expect(screen.getByRole("tab", { name: "实时预览" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("region", { name: "回放预览" })).toBeInTheDocument();
    expect(screen.queryByText("预览区域")).not.toBeInTheDocument();
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

  it("shares external files across providers and route remounts without losing them on refresh", async () => {
    const user = userEvent.setup();
    const cache = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    vi.spyOn(desktopApi, "chooseGameReplayFiles").mockResolvedValue({ items: [external, external], failures: [] });
    vi.spyOn(desktopApi, "inspectGameReplay").mockImplementation(async (_, path) => mapInfo(path));
    const view = renderPage(cache);
    await user.click(screen.getByRole("button", { name: "选择回放文件" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "开始预览" })).toBeEnabled());
    await user.click(screen.getByRole("tab", { name: "在线 o!rdr" }));
    expect(screen.getByRole("button", { name: "提交视频生成" })).toBeEnabled();
    await user.click(screen.getByRole("button", { name: /素材库/ }));
    const library = within(screen.getByRole("dialog"));
    expect(library.getAllByRole("checkbox")).toHaveLength(1);
    await user.click(library.getByRole("button", { name: "刷新素材库" }));
    expect(library.getAllByRole("checkbox")).toHaveLength(1);
    view.unmount();
    renderPage(cache);
    expect(await screen.findByRole("button", { name: "提交视频生成" })).toBeEnabled();
    expect(screen.getByRole("button", { name: /素材库/ })).toHaveTextContent("1");
  });

  it("keeps a valid selection on picker cancellation and reports partial selection failures", async () => {
    const user = userEvent.setup();
    vi.spyOn(desktopApi, "inspectGameReplay").mockImplementation(async (_, path) => mapInfo(path));
    const choose = vi.spyOn(desktopApi, "chooseGameReplayFiles")
      .mockResolvedValueOnce({ items: [external], failures: [{ path: "bad.osr", error: { code: "INVALID", message: "回放损坏" } }] })
      .mockResolvedValueOnce({ items: [], failures: [] });
    renderPage();
    await user.click(screen.getByRole("button", { name: "选择回放文件" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "开始预览" })).toBeEnabled());
    expect(screen.getByText(/bad.osr: 回放损坏/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "选择回放文件" }));
    expect(choose).toHaveBeenCalledTimes(2);
    expect(screen.getByRole("button", { name: "开始预览" })).toBeEnabled();
  });

  it("blocks unsupported modes and unmatched maps, then allows rematching", async () => {
    const user = userEvent.setup();
    vi.spyOn(desktopApi, "listGameMedia").mockResolvedValue([external]);
    const inspect = vi.spyOn(desktopApi, "inspectGameReplay").mockResolvedValue({ ...mapInfo(external.path), ruleset: "mania" });
    renderPage();
    await waitFor(() => expect(inspect).toHaveBeenCalled());
    expect(screen.getByRole("button", { name: "开始预览" })).toBeDisabled();
    inspect.mockResolvedValue({ ...mapInfo(external.path), beatmap_resource_id: null });
    await user.click(screen.getByRole("button", { name: /素材库/ }));
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "刷新素材库" }));
    await user.click(screen.getByRole("button", { name: "关闭素材库" }));
    const panel = within(screen.getByRole("tabpanel"));
    expect(await panel.findByRole("link", { name: "扫描本地谱面" })).toBeInTheDocument();
    inspect.mockResolvedValue(mapInfo(external.path));
    await user.click(panel.getByRole("button", { name: "重新匹配" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "开始预览" })).toBeEnabled());
  });

  it("submits checked files together to Danser with a settings snapshot", async () => {
    const user = userEvent.setup();
    const cache = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    vi.spyOn(desktopApi, "updateSettings").mockImplementation(async (settings) => settings);
    const second = { ...external, path: "C:/Downloads/second.osr" };
    vi.spyOn(desktopApi, "chooseGameReplayFiles").mockResolvedValue({ items: [external, second], failures: [] });
    vi.spyOn(desktopApi, "inspectGameReplay").mockImplementation(async (_, path) => mapInfo(path));
    const enqueue = vi.spyOn(desktopApi, "enqueueDanserRenders").mockResolvedValue([]);
    renderPage(cache);
    await waitFor(() => expect(cache.getQueryData(["settings"])).toBeDefined());
    act(() => cache.setQueryData(["settings"], (current: object) => ({ ...current, replay_export_directory: "C:/videos" })));
    await user.click(screen.getByRole("button", { name: "选择回放文件" }));
    await user.click(screen.getByRole("button", { name: /素材库/ }));
    const library = within(screen.getByRole("dialog"));
    for (const checkbox of library.getAllByRole("checkbox")) await user.click(checkbox);
    await user.click(library.getByRole("button", { name: "关闭素材库" }));
    await user.click(screen.getByRole("tab", { name: "本地 Danser" }));
    const add = screen.getByRole("button", { name: /加入队列.*2 份/ });
    await waitFor(() => expect(add).toBeEnabled());
    await user.click(add);
    await waitFor(() => expect(enqueue).toHaveBeenCalledWith(expect.objectContaining({ replay_paths: [external.path, second.path], preferences: expect.objectContaining({ fps: 60 }) })));
  });

  it("opens the active provider's tasks and searchable library from the toolbar", async () => {
    const user = userEvent.setup();
    vi.spyOn(desktopApi, "listGameMedia").mockResolvedValue([external]);
    vi.spyOn(desktopApi, "inspectGameReplay").mockImplementation(async (_, path) => mapInfo(path));
    renderPage();
    const tasks = screen.getByRole("button", { name: "任务与输出" });
    expect(tasks.closest(".studio-toolbar-actions")).not.toBeNull();
    expect(screen.queryByText("准备好记录精彩了吗？")).not.toBeInTheDocument();
    await user.click(tasks);
    expect(screen.getByRole("dialog", { name: "任务与输出" })).toHaveAttribute("data-dialog-layout", "popover");
    expect(screen.getByText("等待预览或导出")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "关闭任务与输出" }));
    await user.click(screen.getByRole("tab", { name: "本地 Danser" }));
    expect(screen.getAllByRole("button", { name: "任务与输出" })).toHaveLength(1);
    await user.click(screen.getByRole("button", { name: "任务与输出" }));
    expect(await screen.findByText("队列为空")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "关闭任务与输出" }));
    await user.click(screen.getByRole("button", { name: /素材库/ }));
    const library = within(screen.getByRole("dialog", { name: "回放素材库" }));
    expect(screen.getByRole("dialog")).toHaveAttribute("data-dialog-layout", "popover");
    expect(await library.findByRole("checkbox", { name: "勾选 精彩 回放.osr" })).toBeInTheDocument();
    await user.type(library.getByRole("textbox", { name: "搜索回放" }), "无匹配");
    expect(library.queryByRole("checkbox")).not.toBeInTheDocument();
    await user.clear(library.getByRole("textbox", { name: "搜索回放" }));
    expect(library.getByRole("checkbox", { name: "勾选 精彩 回放.osr" })).toBeInTheDocument();
  });

  it("ignores late inspection results from the previously selected replay", async () => {
    const user = userEvent.setup();
    const first = { ...external, path: "C:/old.osr" };
    let resolveOld!: (info: ReplayMapInfo) => void;
    vi.spyOn(desktopApi, "listGameMedia").mockResolvedValue([first]);
    vi.spyOn(desktopApi, "inspectGameReplay").mockImplementation((_, path) => path === first.path ? new Promise((resolve) => { resolveOld = resolve; }) : Promise.resolve(mapInfo(path)));
    vi.spyOn(desktopApi, "chooseGameReplayFiles").mockResolvedValue({ items: [external], failures: [] });
    renderPage();
    await waitFor(() => expect(resolveOld).toBeDefined());
    await user.click(screen.getByRole("button", { name: "选择回放文件" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "开始预览" })).toBeEnabled());
    await act(async () => { resolveOld({ ...mapInfo(first.path), beatmap_title: "Old source" }); });
    expect(within(screen.getByRole("tabpanel")).getByRole("heading", { level: 1 })).toHaveTextContent(external.path);
    expect(screen.queryByText("Old source")).not.toBeInTheDocument();
  });

  it("honors deep links without resetting a later manual selection on refresh", async () => {
    const user = userEvent.setup();
    const original = { ...external, path: "C:/original.osr" };
    vi.spyOn(desktopApi, "listGameMedia").mockResolvedValue([original]);
    vi.spyOn(desktopApi, "inspectGameReplay").mockImplementation(async (_, path) => mapInfo(path));
    vi.spyOn(desktopApi, "chooseGameReplayFiles").mockResolvedValue({ items: [external], failures: [] });
    renderPage(undefined, `/local/media/render?danser=1&replay=${encodeURIComponent(original.path)}`);
    expect(screen.getByRole("tab", { name: "本地 Danser" })).toHaveAttribute("aria-selected", "true");
    await waitFor(() => expect(within(screen.getByRole("tabpanel")).getByRole("heading", { level: 1 })).toHaveTextContent(original.path));
    await user.click(screen.getByRole("button", { name: "选择回放文件" }));
    await user.click(screen.getByRole("button", { name: /素材库/ }));
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "刷新素材库" }));
    await user.click(screen.getByRole("button", { name: "关闭素材库" }));
    expect(within(screen.getByRole("tabpanel")).getByRole("heading", { level: 1 })).toHaveTextContent(external.path);
  });

  it("closes the native preview on provider change and does not auto-start on return", async () => {
    const user = userEvent.setup();
    vi.stubGlobal("ResizeObserver", class { observe() {} disconnect() {} });
    vi.spyOn(desktopApi, "listGameMedia").mockResolvedValue([external]);
    vi.spyOn(desktopApi, "inspectGameReplay").mockImplementation(async (_, path) => mapInfo(path));
    vi.spyOn(desktopApi, "getLocalBeatmapPath").mockResolvedValue("C:/Songs/map.osu");
    const open = vi.spyOn(desktopApi, "liveRenderOpen").mockResolvedValue({ durationMs: 1000 });
    const close = vi.spyOn(desktopApi, "liveRenderClose").mockResolvedValue(undefined);
    renderPage();
    const start = screen.getByRole("button", { name: "开始预览" });
    await waitFor(() => expect(start).toBeEnabled());
    await user.click(start);
    await screen.findByRole("button", { name: "停止" });
    await user.click(screen.getByRole("tab", { name: "本地 Danser" }));
    await waitFor(() => expect(close).toHaveBeenCalled());
    await user.click(screen.getByRole("tab", { name: "实时预览" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "开始预览" })).toBeEnabled());
    expect(open).toHaveBeenCalledTimes(1);
    vi.unstubAllGlobals();
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
      ruleset: "osu", path: "/osr/replay.osr", beatmap_hash: "hash", username: "player", beatmap_id: 1,
      beatmap_resource_id: "123", beatmap_title: "Song", submitted: true,
    });
    vi.spyOn(desktopApi, "liveRenderCheckFfmpeg").mockResolvedValue("ffmpeg version 7");
    vi.spyOn(desktopApi, "liveRenderCheckNvenc").mockResolvedValue([true, true]);
    renderPage();

    const exportButton = await screen.findByRole("button", { name: "导出视频" });
    await waitFor(() => expect(exportButton).toBeEnabled());
    await user.click(exportButton);
    act(() => onProgress!({ phase: "render", frame: 1, total: 10, message: "1/10" }));
    expect(await screen.findByText("正在渲染 1/10 帧")).toBeInTheDocument();

    act(() => onProgress!({ phase: "mux", frame: 10, total: 10, message: "混入音频…" }));
    expect(await within(screen.getByRole("dialog")).findByText("混入音频…")).toBeInTheDocument();

    act(() => onProgress!({ phase: "done", frame: 10, total: 10, message: "/videos/replay.mp4" }));
    expect(await within(screen.getByRole("dialog")).findByText(/导出完成/)).toBeInTheDocument();
    expect(within(screen.getByRole("dialog")).getByText(/\/videos\/replay\.mp4/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "打开所在文件夹" })).toBeInTheDocument();
    expect(screen.queryByText(/正在渲染/)).not.toBeInTheDocument();
  });
});
