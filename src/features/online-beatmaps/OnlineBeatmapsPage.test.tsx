import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Link, MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { OnlineBeatmapsPage } from "./OnlineBeatmapsPage";
import { downloadSession } from "./downloadSession";
import type { OnlineBeatmapset } from "../../shared/types/osu";

const api = vi.hoisted(() => ({ searchOnlineBeatmapsets: vi.fn(), getOnlineBeatmapset: vi.fn(), getOnlineBeatmapBackground: vi.fn(), getOnlineBeatmap: vi.fn(), collectOnlineBeatmapsets: vi.fn(), openExternal: vi.fn(), onBeatmapDownloadProgress: vi.fn(), downloadOnlineBeatmapsets: vi.fn(), cancelOnlineBeatmapDownload: vi.fn() }));
vi.mock("../../shared/lib/tauri", () => ({ desktopApi: api }));
vi.mock("../../app/ModeContext", () => ({ useMode: () => ({ ruleset: "osu" }) }));
vi.mock("../settings/api", () => ({ settingsQueryKey: ["settings"], useSettings: () => ({ data: { beatmap_download_directory: "C:/Maps", default_beatmap_download_provider: "sayobot", reduce_motion: true, preview_volume: 65 } }) }));
vi.mock("../tools/ToolsPage", () => ({ BeatmapPreviewCard: () => <div>Visual preview</div> }));
vi.mock("../../shared/components/StageBackground", () => ({ StageBackground: ({ source }: { source: string }) => <div data-testid="artwork" data-source={source} /> }));
vi.mock("./BeatmapsetDetailDialog", () => ({ BeatmapsetDetailDialog: ({ beatmapsetId, initialBeatmapId }: { beatmapsetId: number | null; initialBeatmapId: number | null }) => beatmapsetId ? <div data-testid="detail-link">{beatmapsetId}/{initialBeatmapId}</div> : null }));

const makeSet = (id: number): OnlineBeatmapset => ({ id, title: `Song ${id}`, artist: "Artist", creator: "Mapper", status: "ranked", covers: { cover: `cover-${id}.jpg` }, beatmaps: [{ id: id * 10, beatmapset_id: id, difficulty_rating: 5, mode: "osu", status: "ranked", total_length: 120, version: "Insane" }] });
function mount(url = "/online/beatmaps", nextLink?: string) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: 0 } } });
  return render(<QueryClientProvider client={client}><MemoryRouter initialEntries={[url]}>{nextLink ? <Link to={nextLink}>Open linked beatmap</Link> : null}<OnlineBeatmapsPage /></MemoryRouter></QueryClientProvider>);
}
beforeEach(() => {
  vi.clearAllMocks(); downloadSession.clear();
  vi.stubGlobal("ResizeObserver", class { observe() {} disconnect() {} });
  api.searchOnlineBeatmapsets.mockResolvedValue({ beatmapsets: Array.from({ length: 50 }, (_, index) => makeSet(index + 1)), total: 200, cursor_string: "next" });
  api.getOnlineBeatmapset.mockImplementation(async (id) => makeSet(id));
  api.getOnlineBeatmapBackground.mockResolvedValue(null);
  api.collectOnlineBeatmapsets.mockResolvedValue({ items: [makeSet(1), makeSet(2), { ...makeSet(3), availability: { download_disabled: true } }], truncated: true });
  api.openExternal.mockResolvedValue(undefined);
  api.onBeatmapDownloadProgress.mockResolvedValue(() => undefined);
});

async function searchFor(text = "music") {
  await userEvent.type(screen.getByRole("combobox", { name: "搜索在线谱面" }), `${text}{Enter}`);
  await screen.findByRole("region", { name: text ? "搜索结果" : "在线谱面" });
  await screen.findByRole("button", { name: "查看 Song 1" });
}

describe("search engine flow", () => {
  it("dismisses autocomplete on search and supports nearby keyboard and mouse navigation", async () => {
    const page = mount(); await searchFor();
    const input = screen.getByRole("combobox", { name: "搜索在线谱面" });
    expect(input).not.toHaveFocus();
    expect(page.container.querySelector("datalist option")).not.toBeInTheDocument();
    fireEvent.keyDown(document, { key: "/" }); expect(input).toHaveFocus();
    expect(page.container.querySelector("datalist option")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "查看 Song 1" }));
    await screen.findByRole("heading", { name: "Song 1" });
    fireEvent.keyDown(document, { key: "j" });
    await screen.findByRole("heading", { name: "Song 2" });
    fireEvent.keyDown(document, { key: "k" });
    await screen.findByRole("heading", { name: "Song 1" });
    fireEvent.mouseDown(window, { button: 3 }); fireEvent.mouseUp(window, { button: 3 });
    expect(page.container.querySelector(".online-stage")).toHaveAttribute("data-view", "results");
    fireEvent.keyDown(document, { key: "ArrowLeft", altKey: true });
    expect(page.container.querySelector(".online-stage")).toHaveAttribute("data-view", "home");
  });
  it("does not navigate while typing, composing text or editing a filter dialog", async () => {
    const page = mount(); await searchFor();
    const input = screen.getByRole("combobox", { name: "搜索在线谱面" });
    input.focus();
    fireEvent.keyDown(input, { key: "Escape" });
    fireEvent.keyDown(input, { key: "ArrowLeft", altKey: true, isComposing: true });
    expect(page.container.querySelector(".online-stage")).toHaveAttribute("data-view", "results");
    await userEvent.click(screen.getByRole("button", { name: "筛选 2" }));
    fireEvent.keyDown(document, { key: "ArrowLeft", altKey: true });
    fireEvent.mouseUp(window, { button: 3 });
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(page.container.querySelector(".online-stage")).toHaveAttribute("data-view", "results");
  });
  it("opens a centered search home with five independently cached trends and no selected beatmap", async () => {
    mount();
    expect(screen.getByRole("heading", { name: "OPP Beatmaps" })).toBeInTheDocument();
    await screen.findByRole("button", { name: "查看 Song 1" });
    expect(screen.getAllByRole("listitem")).toHaveLength(5);
    expect(api.getOnlineBeatmapset).not.toHaveBeenCalled();
    expect(api.getOnlineBeatmapBackground).not.toHaveBeenCalled();
    expect(api.searchOnlineBeatmapsets).toHaveBeenCalledOnce();
    expect(api.searchOnlineBeatmapsets).toHaveBeenCalledWith(expect.objectContaining({ query: "", ruleset: "osu", sort: "favourites_desc", include_nsfw: false, ranked_from: expect.stringMatching(/^\d{4}-\d{2}-\d{2}$/) }));
    expect(screen.queryByRole("toolbar", { name: "谱面排序方式" })).not.toBeInTheDocument();
    expect(screen.queryByLabelText("已应用的筛选条件")).not.toBeInTheDocument();
    await userEvent.type(screen.getByRole("combobox", { name: "搜索在线谱面" }), "draft");
    expect(api.searchOnlineBeatmapsets).toHaveBeenCalledOnce();
  });
  it("searches before selecting, enters a stage on click and restores the result scroll position", async () => {
    const page = mount(); await searchFor();
    expect(screen.queryByRole("heading", { name: "Song 1" })).not.toBeInTheDocument();
    expect(api.getOnlineBeatmapset).not.toHaveBeenCalled();
    const scroll = page.container.querySelector<HTMLDivElement>(".online-results-scroll")!;
    fireEvent.scroll(scroll, { target: { scrollTop: 4000 } });
    await userEvent.click(await screen.findByRole("button", { name: "查看 Song 25" }));
    expect(await screen.findByRole("heading", { name: "Song 25" })).toBeInTheDocument();
    fireEvent.scroll(scroll, { target: { scrollTop: 500 } });
    await userEvent.click(screen.getByRole("button", { name: "返回搜索结果" }));
    await waitFor(() => expect(page.container.querySelector('.online-stage')).toHaveAttribute('data-view', 'results'));
    expect(scroll.scrollTop).toBe(4000);
    expect(screen.getByRole("combobox", { name: "搜索在线谱面" })).toHaveValue("music");
    expect(screen.getAllByRole("listitem").length).toBeLessThan(20);
  });
  it("opens a trend in the stage and returns to the home without issuing a user search", async () => {
    mount(); await userEvent.click(await screen.findByRole("button", { name: "查看 Song 2" }));
    expect(await screen.findByRole("heading", { name: "Song 2" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "近期热门" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "返回热门" }));
    await screen.findByRole("heading", { name: "OPP Beatmaps" });
    expect(api.searchOnlineBeatmapsets).toHaveBeenCalledOnce();
  });
  it("applies input and modal filter drafts together and discards unapplied changes", async () => {
    const user = userEvent.setup(); mount(); await screen.findByRole("button", { name: "查看 Song 1" });
    await user.type(screen.getByRole("combobox", { name: "搜索在线谱面" }), "pending");
    await user.click(screen.getByRole("button", { name: "筛选 2" }));
    expect(screen.getByRole("combobox", { name: "搜索在线谱面" })).toHaveValue("pending");
    await user.click(screen.getByRole("button", { name: "社区喜爱 (Loved)" }));
    await user.click(screen.getByRole("button", { name: "关闭筛选" }));
    expect(api.searchOnlineBeatmapsets).toHaveBeenCalledOnce();
    await user.click(screen.getByRole("button", { name: "筛选 2" }));
    expect(screen.getByRole("button", { name: "上架 (Ranked)" })).toHaveAttribute("aria-pressed", "true");
    await user.click(screen.getByRole("button", { name: "社区喜爱 (Loved)" }));
    await user.click(screen.getByRole("button", { name: "应用并搜索" }));
    await screen.findByRole("region", { name: "搜索结果" });
    expect(api.searchOnlineBeatmapsets).toHaveBeenLastCalledWith(expect.objectContaining({ query: "pending", status: "loved" }));
    await user.type(screen.getByRole("combobox", { name: "搜索在线谱面" }), " draft");
    await user.click(screen.getByRole("button", { name: "按游玩次数排序" }));
    await waitFor(() => expect(api.searchOnlineBeatmapsets).toHaveBeenLastCalledWith(expect.objectContaining({ query: "pending", sort: "plays_desc", status: "loved" })));
    await user.click(screen.getByRole("button", { name: "清空搜索" }));
    await screen.findByRole("region", { name: "在线谱面" });
    expect(api.searchOnlineBeatmapsets).toHaveBeenLastCalledWith(expect.objectContaining({ query: "", sort: "plays_desc", status: "loved" }));
  });
  it("preserves title filters and transitions back to results for same-title searches", async () => {
    mount(); await searchFor(); await userEvent.click(screen.getByRole("button", { name: "查看 Song 1" }));
    await screen.findByRole("heading", { name: "Song 1" });
    await userEvent.click(screen.getByRole("button", { name: "打开当前难度官网" }));
    expect(api.openExternal).toHaveBeenCalledWith("https://osu.ppy.sh/beatmapsets/1#osu/10");
    await userEvent.type(screen.getByRole("combobox", { name: "搜索在线谱面" }), " draft");
    await userEvent.click(screen.getByRole("button", { name: "搜索同名" }));
    await waitFor(() => expect(api.searchOnlineBeatmapsets).toHaveBeenLastCalledWith(expect.objectContaining({ query: "", title: "Song 1", status: "ranked", sort: "relevance_desc" })));
    expect(screen.getByRole("combobox", { name: "搜索在线谱面" })).toHaveValue("");
    expect(screen.getByRole("button", { name: "移除条件 标题 · Song 1" })).toBeInTheDocument();
  });
  it("keeps one audio player without entering the stage when a preview is clicked", async () => {
    const audios: { pause: ReturnType<typeof vi.fn>; onerror: (() => void) | null }[] = [];
    class PreviewAudio {
      volume = 0; onpause: (() => void) | null = null; onended: (() => void) | null = null; onerror: (() => void) | null = null;
      play = vi.fn().mockResolvedValue(undefined); pause = vi.fn(() => this.onpause?.());
      constructor() { audios.push(this); }
    }
    vi.stubGlobal("Audio", PreviewAudio);
    api.searchOnlineBeatmapsets.mockResolvedValue({ beatmapsets: [1, 2].map((id) => ({ ...makeSet(id), preview_url: `//preview.test/${id}.mp3` })), total: 2 });
    const page = mount(); await userEvent.click(await screen.findByRole("button", { name: "试听 Song 2" }));
    expect(screen.getByRole("heading", { name: "OPP Beatmaps" })).toBeInTheDocument();
    expect(api.getOnlineBeatmapset).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "试听 Song 1" }));
    expect(audios[0].pause).toHaveBeenCalled();
    act(() => audios[1].onerror?.()); expect(screen.getByRole("status")).toHaveTextContent("试听加载失败");
    page.unmount(); expect(audios[1].pause).toHaveBeenCalled(); vi.unstubAllGlobals();
  });
  it("shows difficulty details in a portal without moving result rows", async () => {
    mount(); await searchFor();
    const first = screen.getByRole("button", { name: "查看 Song 1" }).closest<HTMLElement>('[role=listitem]')!;
    const second = screen.getByRole("button", { name: "查看 Song 2" }).closest<HTMLElement>('[role=listitem]')!;
    const top = second.style.top;
    await userEvent.hover(first);
    const popover = await screen.findByRole("region", { name: "Song 1 的难度详情" });
    expect(document.body).toContainElement(popover); expect(first).not.toContainElement(popover);
    expect(second.style.top).toBe(top); expect(within(popover).getByText("Insane")).toBeInTheDocument();
    fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("region", { name: "Song 1 的难度详情" })).not.toBeInTheDocument());
    const trigger = within(first).getByRole("button", { name: "1 个难度" });
    fireEvent.keyDown(trigger, { key: "ArrowDown" });
    await waitFor(() => expect(screen.getByRole("region", { name: "Song 1 的难度详情" })).toHaveFocus());
    fireEvent.keyDown(document.activeElement!, { key: "Escape" });
    expect(trigger).toHaveFocus();
    expect(screen.queryByRole("region", { name: "Song 1 的难度详情" })).not.toBeInTheDocument();
  });
  it("keeps download selections across searches and page remounts, with batch actions in More", async () => {
    const user = userEvent.setup(); const page = mount(); await searchFor();
    expect(screen.getByLabelText("更多结果操作").closest("details")).not.toHaveAttribute("open");
    await user.click(screen.getByLabelText("更多结果操作")); await user.click(screen.getByRole("button", { name: "多选" }));
    await user.click(screen.getByRole("checkbox", { name: "选择 Song 1" })); await user.click(screen.getByRole("button", { name: "加入下载清单" }));
    expect(screen.getByRole("button", { name: "下载清单 · 1" })).toBeInTheDocument();
    await user.click(screen.getByLabelText("更多结果操作")); await user.selectOptions(screen.getByRole("combobox", { name: "批量收集数量" }), "50");
    await user.click(screen.getByRole("button", { name: "加入前 50 首" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "下载清单 · 2" })).toBeInTheDocument());
    expect(api.collectOnlineBeatmapsets).toHaveBeenCalledWith(expect.objectContaining({ query: "music", cursor_string: null }), 50);
    page.unmount(); mount(); await user.click(screen.getByRole("button", { name: "下载清单 · 2" }));
    expect(within(screen.getByRole("dialog")).getByText("Song 1")).toBeInTheDocument();
  });
  it("keeps loaded results when the next page fails", async () => {
    api.searchOnlineBeatmapsets.mockImplementation(async (query) => {
      if (query.cursor_string) throw new Error("next page failed");
      return { beatmapsets: [makeSet(1)], total: 2, cursor_string: "next" };
    });
    mount("/online/beatmaps?query=test"); await screen.findByText("next page failed");
    expect(screen.getByRole("button", { name: "查看 Song 1" })).toBeInTheDocument();
    api.searchOnlineBeatmapsets.mockResolvedValueOnce({ beatmapsets: [makeSet(2)], total: 2 });
    await userEvent.click(screen.getByRole("button", { name: "重试" }));
    expect(await screen.findByRole("button", { name: "查看 Song 2" })).toBeInTheDocument();
  });
  it("allows a search when the independent trend request fails", async () => {
    api.searchOnlineBeatmapsets.mockImplementation(async (query) => {
      if (query.sort === "favourites_desc") throw new Error("trend unavailable");
      return { beatmapsets: [makeSet(1)], total: 1 };
    });
    mount(); await screen.findByRole("button", { name: "重试热门" });
    await searchFor("working");
    expect(screen.getByRole("button", { name: "查看 Song 1" })).toBeInTheDocument();
  });
  it("keeps the latest search when an earlier request returns late", async () => {
    let resolveFirst!: (value: unknown) => void;
    api.searchOnlineBeatmapsets.mockImplementation((query) => query.query === "first"
      ? new Promise((resolve) => { resolveFirst = resolve; })
      : Promise.resolve({ beatmapsets: [makeSet(2)], total: 1 }));
    mount();
    const input = screen.getByRole("combobox", { name: "搜索在线谱面" });
    await userEvent.type(input, "first{Enter}");
    await waitFor(() => expect(resolveFirst).toBeTypeOf("function"));
    await userEvent.clear(input); await userEvent.type(input, "last{Enter}");
    await screen.findByRole("button", { name: "查看 Song 2" });
    await act(async () => resolveFirst({ beatmapsets: [makeSet(1)], total: 1 }));
    expect(screen.queryByRole("button", { name: "查看 Song 1" })).not.toBeInTheDocument();
    expect(input).toHaveValue("last");
  });
  it("resolves a beatmap-only deep link before selecting its difficulty", async () => {
    api.getOnlineBeatmap.mockResolvedValue({ beatmapset_id: 99 });
    mount("/online/beatmaps?beatmap=990");
    expect(await screen.findByText("BID 990")).toBeInTheDocument();
    expect(api.getOnlineBeatmap).toHaveBeenCalledWith(990);
  });
  it("handles an incoming deep link while the online page is already mounted", async () => {
    mount("/online/beatmaps", "/online/beatmaps?beatmapset=99&beatmap=990");
    await screen.findByRole("heading", { name: "OPP Beatmaps" });
    await userEvent.click(screen.getByRole("link", { name: "Open linked beatmap" }));
    expect(await screen.findByText("BID 990")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Song 99" })).toBeInTheDocument();
  });
  it("opens deep links directly in the stage with the requested difficulty", async () => {
    api.getOnlineBeatmapset.mockResolvedValue({ ...makeSet(99), beatmaps: [{ ...makeSet(99).beatmaps![0], id: 999 }] });
    mount("/online/beatmaps?beatmapset=99&beatmap=999");
    expect(await screen.findByText("BID 999")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Song 99" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "OPP Beatmaps" })).not.toBeInTheDocument();
  });
  it("ignores late details when selecting another beatmap", async () => {
    let resolveFirst!: (value: OnlineBeatmapset) => void;
    api.getOnlineBeatmapset.mockImplementation((id) => id === 1 ? new Promise((resolve) => { resolveFirst = resolve; }) : Promise.resolve(makeSet(id)));
    mount(); await searchFor(); await userEvent.click(screen.getByRole("button", { name: "查看 Song 1" }));
    await screen.findByRole("heading", { name: "Song 1" }); await userEvent.click(screen.getByRole("button", { name: "查看 Song 2" }));
    await screen.findByRole("heading", { name: "Song 2" });
    await act(async () => resolveFirst({ ...makeSet(1), title: "Late response" }));
    expect(screen.queryByRole("heading", { name: "Late response" })).not.toBeInTheDocument();
  });
});
