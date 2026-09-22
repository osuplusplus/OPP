import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes, useLocation, useNavigate } from "react-router-dom";
import { beforeEach, expect, it, vi } from "vitest";
import { desktopApi } from "../../shared/lib/tauri";
import type { CollectionBrowseRow, CollectionFolderSummary } from "../../shared/types/osu";
import { stageSet } from "../local-analysis/stageFixtures.test-data";
import { CollectionExplorer } from "./CollectionExplorer";
import { CollectionFolderTile } from "./CollectionFolderTile";
import { CollectionRecordDrawer } from "./CollectionRecordDrawer";
import { CollectionBeatmapRow } from "./CollectionBeatmapRow";

vi.mock("../../shared/lib/tauri", async (original) => {
  const actual = await original<typeof import("../../shared/lib/tauri")>();
  return { ...actual, desktopApi: { ...actual.desktopApi,
    getCollectionArtwork: vi.fn().mockResolvedValue([]),
    openLazerBeatmap: vi.fn().mockResolvedValue(undefined),
    refreshLocalScores: vi.fn().mockResolvedValue({ players: ["Player"], default_player: "Player", count: 1, errors: [] }),
    queryCollectionBrowser: vi.fn(), getCollectionEntryScores: vi.fn().mockResolvedValue([]),
    saveCollectionRecord: vi.fn(), getCollectionRecord: vi.fn(), getLocalBeatmapBackground: vi.fn().mockResolvedValue(null),
  } };
});
const folder: CollectionFolderSummary = { id: "folder", name: "练习收藏", creator: "Player", source: "opp", read_only: false, pending_write: false, entry_count: 2000, missing_count: 0, beatmapset_count: 300, revision: 1 };
const row: CollectionBrowseRow = { key: "folder:entry:0", folder_id: "folder", folder_name: folder.name, read_only: false,
  entry: { id: "entry", beatmap_id: 502, beatmapset_id: 456, checksum: "md5", ruleset: "osu", difficulty_name: "Insane", title: "Song", artist: "Artist", creator: "Mapper", resolved: true }, local: stageSet.difficulties[0], slot: "NM2", source_slot: "NM2", selected_by: "", pool_comment: "",
  record: { revision: 0, tags: [], note: "这里容易失误", slot_override: null, scores: [], representative: null }, latest_score: null, representative_available: true, matches: [{ field: "笔记", text: "这里容易失误" }] };
function createClient() { return new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: 0 } } }); }
function LocalPage() { const location = useLocation(); const navigate = useNavigate(); return <><p>{location.search}</p><button onClick={() => navigate(-1)}>返回收藏</button></>; }
beforeEach(() => {
  sessionStorage.clear(); vi.clearAllMocks(); vi.spyOn(window, "scrollTo").mockImplementation(() => undefined);
  vi.mocked(desktopApi.queryCollectionBrowser).mockImplementation(async (q) => ({ items: [row], total: 2000, offset: q.offset, limit: 50, matching_folders: [], pool: null }));
  vi.mocked(desktopApi.saveCollectionRecord).mockImplementation(async (_folder, _entry, record) => ({ ...record, revision: record.revision + 1 }));
});
function explorer(initial = "/collections") {
  const client = createClient();
  render(<QueryClientProvider client={client}><MemoryRouter initialEntries={[initial]}><Routes><Route path="/collections" element={<CollectionExplorer folders={[folder]} loading={false} failed={false} toolbar={null} onRetry={vi.fn()} onChanged={vi.fn()} onDownload={vi.fn()} onNotice={vi.fn()} />} /><Route path="/local/maps" element={<LocalPage />} /></Routes></MemoryRouter></QueryClientProvider>);
  return client;
}
it("opens a folder, pages on the backend, and returns to the same page after exact navigation", async () => {
  const user = userEvent.setup(); const client = explorer();
  expect(desktopApi.queryCollectionBrowser).not.toHaveBeenCalled();
  expect(screen.getByRole("heading", { name: "谱面收藏夹" })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "打开收藏夹 练习收藏" }));
  await screen.findByRole("button", { name: "打开谱面 Song Insane" });
  expect(screen.queryByRole("heading", { name: "谱面收藏夹" })).not.toBeInTheDocument();
  expect(document.querySelector(".collection-toolbar")).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "下一页" }));
  await waitFor(() => expect(desktopApi.queryCollectionBrowser).toHaveBeenLastCalledWith(expect.objectContaining({ folder_id: "folder", offset: 50 })));
  await user.click(screen.getByRole("button", { name: "打开谱面 Song Insane" }));
  expect(await screen.findByText(/resource=502/)).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "返回收藏" }));
  expect(await screen.findByText("51–100 / 2000")).toBeInTheDocument(); client.clear();
});

it("shows saved online parameters and a single missing-download badge without requiring local files", () => {
  const client = createClient();
  render(<QueryClientProvider client={client}><CollectionBeatmapRow row={{ ...row, local: null, metadata: {
    id: 502, beatmapset_id: 456, mode: "osu", status: "ranked", version: "Insane", difficulty_rating: 6.25,
    bpm: 175, total_length: 164, ar: 9.5, accuracy: 9, cs: 3.8, drain: 5, max_combo: 1000,
    count_circles: 300, count_sliders: 150, count_spinners: 1,
  } }} search="" onOpen={vi.fn()} onRecord={vi.fn()} onDetail={vi.fn()} onNavigate={vi.fn()} onChanged={vi.fn()} onDownload={vi.fn()} onNotice={vi.fn()} /></QueryClientProvider>);
  const stats = screen.getByLabelText("谱面 NM 参数");
  expect(stats).toHaveTextContent("6.25 ★"); expect(stats).toHaveTextContent("175 BPM");
  expect(stats).toHaveTextContent("2:44"); expect(stats).toHaveTextContent("AR9.5");
  expect(stats).toHaveTextContent("451 物件"); expect(stats).toHaveTextContent("FC 1,000x");
  expect(screen.getAllByText("未下载")).toHaveLength(1);
  expect(screen.queryByText(/参数暂不可用/)).not.toBeInTheDocument();
  expect(screen.getByLabelText("图位 NM2").querySelector("strong")).toHaveTextContent("NM2");
  expect(document.querySelector(".collection-thumbnail img")).toHaveAttribute("src", "https://assets.ppy.sh/beatmaps/456/covers/list.jpg");
  client.clear();
});
it("searches globally inside a folder, shows comment hits and restores browsing on clear", async () => {
  const user = userEvent.setup(); const client = explorer("/collections?folder=folder&page=2");
  await screen.findByText("101–150 / 2000");
  await user.type(screen.getByRole("textbox", { name: "搜索全部收藏" }), "失误");
  await waitFor(() => expect(desktopApi.queryCollectionBrowser).toHaveBeenLastCalledWith(expect.objectContaining({ search: "失误", offset: 0 })));
  await waitFor(() => expect(document.querySelector("mark")?.textContent).toBe("失误"));
  await user.click(screen.getByRole("button", { name: "清空搜索" }));
  expect(await screen.findByText("101–150 / 2000")).toBeInTheDocument(); client.clear();
});
it("delays folder previews and cancels a quick pointer pass", async () => {
  vi.useFakeTimers(); const client = createClient();
  const { unmount } = render(<QueryClientProvider client={client}><CollectionFolderTile folder={folder} onOpen={vi.fn()} /></QueryClientProvider>);
  const tile = screen.getByRole("button", { name: "打开收藏夹 练习收藏" });
  fireEvent.mouseEnter(tile); await act(() => vi.advanceTimersByTimeAsync(300)); fireEvent.mouseLeave(tile); await act(() => vi.advanceTimersByTimeAsync(500));
  expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
  fireEvent.focus(tile); await act(() => vi.advanceTimersByTimeAsync(450)); expect(screen.getByRole("tooltip")).toBeInTheDocument();
  unmount(); client.clear(); vi.useRealTimers();
});
it("shows tournament context and NM stats without routing record or menu clicks", async () => {
  const user = userEvent.setup(); const client = createClient(); const open = vi.fn(); const record = vi.fn();
  render(<QueryClientProvider client={client}><CollectionBeatmapRow row={{ ...row, selected_by: "选图选手", pool_comment: "尾段控指，适合保底" }} search="" onOpen={open} onRecord={record} onDetail={vi.fn()} onNavigate={vi.fn()} onChanged={vi.fn()} onDownload={vi.fn()} onNotice={vi.fn()} /></QueryClientProvider>);
  expect(screen.getByLabelText("谱面 NM 参数")).toHaveTextContent("AR9");
  expect(screen.getByText("尾段控指，适合保底")).toBeInTheDocument();
  expect(screen.getByText("这里容易失误")).toBeInTheDocument();
  expect(screen.getByText("选图 · 选图选手")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "编辑 Song 的标签和笔记" }));
  expect(record).toHaveBeenCalledOnce();
  await user.click(screen.getByLabelText("Song 更多操作"));
  expect(open).not.toHaveBeenCalled();
  client.clear();
});
it("shows the server's full-folder overview rather than deriving ranges from five preview rows", async () => {
  const user = userEvent.setup(); const client = createClient();
  vi.mocked(desktopApi.queryCollectionBrowser).mockResolvedValue({ items: [row], total: 2000, offset: 0, limit: 5, matching_folders: [], pool: null, overview: { local_count: 1900, marked_count: 42, noted_count: 16, stars: [2.4, 8.9], bpm: [120, 240], total_length_ms: 3600000, slots: [["NM", 100], ["LZ", 8]] } });
  render(<QueryClientProvider client={client}><CollectionFolderTile folder={folder} onOpen={vi.fn()} /></QueryClientProvider>);
  await user.hover(screen.getByRole("button", { name: "打开收藏夹 练习收藏" }));
  const tooltip = await screen.findByRole("tooltip");
  expect(await within(tooltip).findByText("2.4–8.9 ★")).toBeInTheDocument();
  expect(within(tooltip).getByText("1900")).toBeInTheDocument();
  expect(within(tooltip).getByText("已标记 42 / 2000 · 已写笔记 16 / 2000")).toBeInTheDocument();
  client.clear();
});
it("keeps failed notebook drafts, retries and does not call membership writes", async () => {
  const user = userEvent.setup(); const client = createClient(); const closed = vi.fn();
  vi.mocked(desktopApi.saveCollectionRecord).mockRejectedValueOnce({ message: "磁盘写入失败" });
  render(<QueryClientProvider client={client}><CollectionRecordDrawer row={row} player="Player" onClose={closed} /></QueryClientProvider>);
  fireEvent.change(screen.getByRole("textbox", { name: "我的笔记" }), { target: { value: "第二段容易失误" } });
  expect(await screen.findByText(/保存失败：磁盘写入失败/)).toBeInTheDocument();
  expect(sessionStorage.getItem("opp:collection-draft:folder:entry")).toContain("第二段容易失误");
  await user.click(screen.getByRole("button", { name: "重试保存" }));
  expect(await screen.findByText("已保存到 OPP")).toBeInTheDocument();
  expect(desktopApi.saveCollectionRecord).toHaveBeenLastCalledWith("folder", "entry", expect.objectContaining({ note: "第二段容易失误" }));
  await user.click(screen.getByRole("button", { name: "关闭记录" })); expect(closed).toHaveBeenCalled(); client.clear();
});

it("resolves a record conflict by explicitly discarding the draft and loading the saved revision", async () => {
  const user = userEvent.setup(); const client = createClient();
  vi.mocked(desktopApi.saveCollectionRecord).mockRejectedValueOnce({ message: "记录已更新" });
  vi.mocked(desktopApi.getCollectionRecord).mockResolvedValue({ ...row.record, revision: 7, note: "另一个窗口已保存" });
  render(<QueryClientProvider client={client}><CollectionRecordDrawer row={row} player="Player" onClose={vi.fn()} /></QueryClientProvider>);
  fireEvent.change(screen.getByRole("textbox", { name: "我的笔记" }), { target: { value: "冲突草稿" } });
  await screen.findByText(/保存失败：记录已更新/);
  await user.click(screen.getByRole("button", { name: "丢弃草稿并重载" }));
  await waitFor(() => expect(screen.getByRole("textbox", { name: "我的笔记" })).toHaveValue("另一个窗口已保存"));
  expect(sessionStorage.getItem("opp:collection-draft:folder:entry")).not.toContain("冲突草稿");
  fireEvent.change(screen.getByRole("textbox", { name: "我的笔记" }), { target: { value: "基于新版本继续编辑" } });
  await screen.findByText("已保存到 OPP");
  expect(desktopApi.saveCollectionRecord).toHaveBeenLastCalledWith("folder", "entry", expect.objectContaining({ revision: 7, note: "基于新版本继续编辑" }));
  client.clear();
});

it("opens the exact BID in lazer even without a local map and reports launch errors", async () => {
  const client = createClient(); const notice = vi.fn(); const open = vi.fn();
  render(<QueryClientProvider client={client}><CollectionBeatmapRow row={{ ...row, local: null, is_custom: true, is_original: true }} search="" onOpen={open} onRecord={vi.fn()} onDetail={vi.fn()} onNavigate={vi.fn()} onChanged={vi.fn()} onDownload={vi.fn()} onNotice={notice} /></QueryClientProvider>);
  expect(screen.getByText("比赛定制 · 原创")).toBeInTheDocument();
  vi.mocked(desktopApi.openLazerBeatmap).mockRejectedValueOnce({ message: "未找到 lazer" });
  await userEvent.click(screen.getByRole("button", { name: "在 lazer 中打开" }));
  expect(desktopApi.openLazerBeatmap).toHaveBeenCalledWith(502);
  expect(notice).toHaveBeenCalledWith("未找到 lazer"); expect(open).not.toHaveBeenCalled(); client.clear();
});
