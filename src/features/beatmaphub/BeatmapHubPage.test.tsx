import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { BeatmapHubPack } from "../../shared/types/osu";

const api = vi.hoisted(() => Object.fromEntries([
  "getBeatmapHubAuthStatus", "getAuthStatus", "listCollectionSummaries", "getBeatmapHubProfile", "previewBeatmapHubPack", "getOnlineBeatmapset", "getBeatmapHubRecommendations", "searchBeatmapHubPacks", "getBeatmapHubComments", "getSettings", "getLocalSources", "onBeatmapDownloadProgress", "createBeatmapHubProfile", "linkBeatmapHubDevice", "loginBeatmapHub", "logoutBeatmapHub", "createBeatmapHubDeviceLink", "revokeBeatmapHubDevice", "publishBeatmapHubPack", "updateBeatmapHubPack", "deleteBeatmapHubPack", "favoriteBeatmapHubPack", "likeBeatmapHubPack", "rateBeatmapHubPack", "createBeatmapHubComment", "updateBeatmapHubComment", "deleteBeatmapHubComment", "importCollectionArchive", "importBeatmapHubPack", "beginCollectionTask", "getCollectionDownloadItems", "downloadOnlineBeatmapsets", "installCollectionDownloads", "getOsuCollectorCollections", "getOsuCollectorCollection", "importOsuCollectorCollection", "onOsuCollectorImportProgress", "getRomaiPoolSummaries", "getTournamentPool", "syncTournamentPoolCollection",
].map((name) => [name, vi.fn()])));
vi.mock("../../shared/lib/tauri", () => ({ desktopApi: api, isTauri: () => true }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
import { BeatmapHubPage } from "./BeatmapHubPage";

const pack: BeatmapHubPack = { id: "7K3N9A", title: "Tech Pack", description: "Practice", is_private: false, owner: { id: "user", display_name: "Player" }, beatmapset_ids: [123], manifest_hash: "hash", rating: { average: 4.5, count: 2 }, likes: { count: 2 }, comments: { count: 0 }, viewer: null, created_at: "2026-01-01", updated_at: "2026-01-02" };
const preview = { pack, locally_available_ids: [], missing_ids: [123] };
function renderPage(client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })) {
  return { ...render(<QueryClientProvider client={client}><MemoryRouter><BeatmapHubPage /></MemoryRouter></QueryClientProvider>), client };
}
async function openPack() {
  await showCommunity();
  await userEvent.click(await screen.findByRole("button", { name: "查看曲包 Tech Pack" }));
  await screen.findByRole("heading", { name: "Tech Pack" });
  await waitFor(() => expect(screen.getByRole("button", { name: "导入收藏夹" })).toBeEnabled());
}
/** 默认来源为 osu!Collector；社区曲包相关用例先切换到社区页签（来源分段控件）。 */
async function showCommunity() {
  await userEvent.click(await screen.findByRole("tab", { name: "社区曲包" }));
}
function connect() {
  api.getBeatmapHubAuthStatus.mockResolvedValue({ has_identity: true, connected: true, display_name: "Player", user_id: "user", device_name: "PC" });
  api.previewBeatmapHubPack.mockResolvedValue({ ...preview, pack: { ...pack, viewer: { rating: 5, favorited: true, can_edit: true } } });
}
describe("BeatmapHubPage", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    api.getBeatmapHubAuthStatus.mockResolvedValue({ has_identity: false, connected: false, device_name: "TEST-PC" });
    api.getAuthStatus.mockResolvedValue({ connected: true, username: "L1rics" });
    api.listCollectionSummaries.mockResolvedValue({ folders: [{ id: "folder", name: "Weekly Practice", beatmapset_count: 1, read_only: false, source: "opp" }], sources: [] });
    api.getBeatmapHubRecommendations.mockResolvedValue([pack]);
    api.searchBeatmapHubPacks.mockResolvedValue([]);
    api.previewBeatmapHubPack.mockResolvedValue(preview);
    api.getBeatmapHubProfile.mockResolvedValue({ user: { id: "user", display_name: "Player" }, current_device_id: "device", devices: [] });
    api.getOnlineBeatmapset.mockResolvedValue({ id: 123, title: "Map", artist: "Artist", creator: "Mapper", status: "ranked", beatmaps: [{ id: 1, mode: "osu", difficulty_rating: 5 }] });
    api.getBeatmapHubComments.mockResolvedValue([]);
    api.getSettings.mockResolvedValue({ default_beatmap_download_provider: "sayobot", include_video_in_beatmap_downloads: false });
    api.getLocalSources.mockResolvedValue([]);
    api.onBeatmapDownloadProgress.mockResolvedValue(() => undefined);
    api.importBeatmapHubPack.mockResolvedValue({ folder_id: "imported-folder", imported_sets: 1, imported_entries: 1, unresolved_sets: 0 });
    api.beginCollectionTask.mockResolvedValue(undefined);
    api.getCollectionDownloadItems.mockResolvedValue([{ beatmapset_id: 123, title: "Map", artist: "Artist" }]);
    api.installCollectionDownloads.mockResolvedValue({ installed_sets: 1, resolved_entries: 1, unresolved_entries: 0 });
    api.publishBeatmapHubPack.mockResolvedValue({ id: "7K3N9A", included: 1, skipped: 0 });
    api.getOsuCollectorCollections.mockResolvedValue({ collections: [], next_cursor: null, has_more: false, total: null });
    api.onOsuCollectorImportProgress.mockResolvedValue(() => undefined);
    api.getRomaiPoolSummaries.mockResolvedValue({ pools: [], fetched_at: 1790000000, from_cache: false });
    api.getTournamentPool.mockResolvedValue(null);
    api.syncTournamentPoolCollection.mockResolvedValue(null);
  });

  it("shows discovery for guests without parsing packs or showing identity setup", async () => {
    renderPage();
    await showCommunity();
    expect(await screen.findByRole("button", { name: "查看曲包 Tech Pack" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "连接 BeatmapHub" })).not.toBeInTheDocument();
    expect(api.getOnlineBeatmapset).not.toHaveBeenCalled();
  });
  it("reloads recommendations on remount without retaining the previous list", async () => {
    const page = renderPage();
    await showCommunity();
    await screen.findByRole("button", { name: "查看曲包 Tech Pack" });
    page.unmount();
    await waitFor(() => expect(page.client.getQueryData(["beatmaphub", "recommendations"])).toBeUndefined());
    api.getBeatmapHubRecommendations.mockResolvedValue([]);
    renderPage(page.client);
    await showCommunity();
    await screen.findByText("社区还没有公开曲包");
    expect(api.getBeatmapHubRecommendations).toHaveBeenCalledTimes(2);
    expect(screen.queryByRole("button", { name: "查看曲包 Tech Pack" })).not.toBeInTheDocument();
  });
  it("shows refresh errors instead of falling back to the previous list", async () => {
    renderPage();
    await showCommunity();
    await screen.findByRole("button", { name: "查看曲包 Tech Pack" });
    api.getBeatmapHubRecommendations.mockRejectedValue(new Error("offline"));
    await userEvent.click(screen.getByRole("button", { name: "刷新推荐" }));
    await screen.findByText("曲包暂时无法加载");
    expect(screen.queryByRole("button", { name: "查看曲包 Tech Pack" })).not.toBeInTheDocument();
  });
  it("opens automatic device connection on demand without legacy profile setup", async () => {
    renderPage();
    await userEvent.click(screen.getByRole("button", { name: "连接身份" }));
    expect(await screen.findByRole("heading", { name: "连接 BeatmapHub" })).toBeInTheDocument();
    expect(screen.getByText("osu! 已登录，PackHub 尚未完成设备认证。")).toBeInTheDocument();
    expect(screen.queryByPlaceholderText("粘贴旧设备生成的 43 位链接码")).not.toBeInTheDocument();
  });
  it("allows guests to preview and import without download configuration", async () => {
    renderPage(); await openPack();
    expect(screen.getByRole("checkbox", { name: "同时下载缺失谱面" })).toBeDisabled();
    await userEvent.click(screen.getByRole("button", { name: "导入收藏夹" }));
    expect(await screen.findByRole("button", { name: "已导入收藏夹" })).toBeDisabled();
    expect(api.importBeatmapHubPack).toHaveBeenCalledTimes(1);
    expect(api.downloadOnlineBeatmapsets).not.toHaveBeenCalled();
  });
  it("keeps keyword search available when recommendations fail and restores discovery when cleared", async () => {
    api.getBeatmapHubRecommendations.mockRejectedValue(new Error("offline"));
    renderPage();
    await showCommunity();
    await userEvent.type(screen.getByRole("textbox", { name: "搜索曲包或输入分享码" }), "tech{Enter}");
    expect(await screen.findByText("没有找到匹配的曲包")).toBeInTheDocument();
    expect(api.searchBeatmapHubPacks).toHaveBeenCalledWith("tech");
    await userEvent.click(screen.getByRole("button", { name: "清空搜索" }));
    expect(await screen.findByText("曲包暂时无法加载")).toBeInTheDocument();
  });
  it("opens case-insensitive share codes through the unified input", async () => {
    renderPage();
    await userEvent.type(screen.getByRole("textbox", { name: "搜索曲包或输入分享码" }), "bph-7k3n9a{Enter}");
    expect(await screen.findByRole("heading", { name: "Tech Pack" })).toBeInTheDocument();
    expect(api.previewBeatmapHubPack).toHaveBeenCalledWith("7K3N9A");
    expect(api.searchBeatmapHubPacks).not.toHaveBeenCalled();
  });
  it("offers keyword search after a bare code cannot be opened", async () => {
    api.previewBeatmapHubPack.mockRejectedValue(new Error("not found"));
    renderPage();
    await userEvent.type(screen.getByRole("textbox", { name: "搜索曲包或输入分享码" }), "7K3N9A{Enter}");
    await userEvent.click(await screen.findByRole("button", { name: "改为关键词搜索" }));
    await waitFor(() => expect(api.searchBeatmapHubPacks).toHaveBeenCalledWith("7K3N9A"));
  });
  it("waits for metadata and imports placeholders if resolution fails", async () => {
    let reject!: (error: Error) => void;
    api.getOnlineBeatmapset.mockReturnValue(new Promise((_, fail) => { reject = fail; }));
    renderPage();
    await showCommunity();
    await userEvent.click(await screen.findByRole("button", { name: "查看曲包 Tech Pack" }));
    expect(await screen.findByRole("button", { name: "导入收藏夹" })).toBeDisabled();
    await act(async () => reject(new Error("metadata unavailable")));
    await waitFor(() => expect(screen.getByRole("button", { name: "导入收藏夹" })).toBeEnabled());
    expect(screen.getByText(/将作为占位条目导入/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "导入收藏夹" }));
    await waitFor(() => expect(api.importBeatmapHubPack).toHaveBeenCalledWith("7K3N9A", []));
  });
  it("does not offer downloading when all sets are local", async () => {
    api.previewBeatmapHubPack.mockResolvedValue({ ...preview, locally_available_ids: [123], missing_ids: [] });
    api.getLocalSources.mockResolvedValue([{ client: "stable", valid: true, install_root: "C:\\osu" }]);
    renderPage(); await openPack();
    expect(screen.getByRole("checkbox")).not.toBeChecked();
    expect(screen.getByText("本地已包含全部谱面集，无需下载。")).toBeInTheDocument();
  });
  it("retries downloading into the same collection after closing and reopening the detail", async () => {
    api.getLocalSources.mockResolvedValue([{ client: "stable", valid: true, install_root: "C:\\osu" }]);
    api.downloadOnlineBeatmapsets.mockRejectedValueOnce(new Error("download offline")).mockResolvedValue({ completed: 1, failed: 0, cancelled: false, completed_paths: ["map.osz"] });
    renderPage(); await openPack();
    await waitFor(() => expect(screen.getByRole("checkbox")).toBeChecked());
    await userEvent.click(screen.getByRole("button", { name: "导入收藏夹" }));
    await screen.findByRole("button", { name: "重试补齐" });
    await userEvent.click(screen.getByRole("button", { name: "关闭曲包详情" }));
    await userEvent.click(screen.getByRole("button", { name: "查看曲包 Tech Pack" }));
    await userEvent.click(await screen.findByRole("button", { name: "重试补齐" }));
    await screen.findByRole("button", { name: "已导入收藏夹" });
    expect(api.importBeatmapHubPack).toHaveBeenCalledTimes(1);
    expect(api.installCollectionDownloads).toHaveBeenCalledWith(["imported-folder"], ["map.osz"]);
  });
  it("keeps publishing successful if the clipboard fails", async () => {
    connect(); const user = userEvent.setup();
    vi.spyOn(navigator.clipboard, "writeText").mockRejectedValue(new Error("clipboard denied"));
    renderPage(); await user.click(await screen.findByRole("button", { name: "发布曲包" }));
    await user.selectOptions(await screen.findByRole("combobox", { name: "本地收藏夹" }), "folder");
    expect(screen.getByRole("textbox", { name: "曲包标题" })).toHaveValue("Weekly Practice");
    await user.click(screen.getByRole("button", { name: "发布并复制分享码" }));
    expect(await screen.findByRole("heading", { name: "曲包已发布" })).toBeInTheDocument();
    expect(await screen.findByText(/无法自动复制，请手动复制下方分享码/)).toBeInTheDocument();
    expect(api.publishBeatmapHubPack).toHaveBeenCalledTimes(1);
  });
  it("preserves a publish draft through identity connection without auto-publishing", async () => {
    api.loginBeatmapHub.mockImplementation(async () => {
      api.getBeatmapHubAuthStatus.mockResolvedValue({ has_identity: true, connected: true, display_name: "Player", user_id: "user", device_name: "PC" });
    });
    renderPage();
    await userEvent.click(screen.getByRole("button", { name: "发布曲包" }));
    await userEvent.selectOptions(await screen.findByRole("combobox"), "folder");
    await userEvent.type(screen.getByRole("textbox", { name: /说明/ }), "Keep my draft");
    await userEvent.click(screen.getByRole("button", { name: "连接身份后继续" }));
    await userEvent.click(await screen.findByRole("button", { name: "重新连接" }));
    await screen.findByRole("button", { name: "发布并复制分享码" });
    expect(screen.getByRole("textbox", { name: /说明/ })).toHaveValue("Keep my draft");
    expect(api.publishBeatmapHubPack).not.toHaveBeenCalled();
  });
  it("connects deletion and requires confirmation", async () => {
    connect(); api.deleteBeatmapHubPack.mockResolvedValue(undefined);
    renderPage(); await openPack();
    await userEvent.click(screen.getByRole("button", { name: "删除曲包" }));
    expect(api.deleteBeatmapHubPack).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "确认删除" }));
    await waitFor(() => expect(api.deleteBeatmapHubPack).toHaveBeenCalledWith("7K3N9A"));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });
  it("requires an explicit source collection before updating an owned pack", async () => {
    connect(); api.updateBeatmapHubPack.mockResolvedValue(undefined);
    renderPage(); await openPack();
    await userEvent.click(screen.getByRole("button", { name: "编辑曲包" }));
    expect(screen.getByRole("button", { name: "确认替换并更新" })).toBeDisabled();
    await userEvent.selectOptions(screen.getByRole("combobox", { name: "本地收藏夹" }), "folder");
    expect(screen.getByRole("textbox", { name: "曲包标题" })).toHaveValue("Tech Pack");
    await userEvent.click(screen.getByRole("button", { name: "确认替换并更新" }));
    await screen.findByRole("heading", { name: "曲包已更新" });
    expect(api.updateBeatmapHubPack).toHaveBeenCalledWith("7K3N9A", "folder", "Tech Pack", "Practice", false);
  });
  it("retains comment drafts across identity prompts and isolates them by pack", async () => {
    const other = { ...pack, id: "ABCD23", title: "Second Pack" };
    api.getBeatmapHubRecommendations.mockResolvedValue([pack, other]);
    api.previewBeatmapHubPack.mockImplementation(async (id: string) => ({ ...preview, pack: id === pack.id ? pack : other }));
    renderPage(); await openPack();
    await userEvent.type(screen.getByRole("textbox", { name: "写评论" }), "My saved draft");
    await userEvent.click(screen.getByRole("button", { name: "连接身份后发送" }));
    await screen.findByRole("heading", { name: "连接 BeatmapHub" });
    await userEvent.keyboard("{Escape}");
    expect(screen.getByRole("textbox", { name: "写评论" })).toHaveValue("My saved draft");
    await userEvent.click(screen.getByRole("button", { name: "关闭曲包详情" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "查看曲包 Tech Pack" })).toHaveFocus());
    await userEvent.click(screen.getByRole("button", { name: "查看曲包 Second Pack" }));
    expect(await screen.findByRole("textbox", { name: "写评论" })).toHaveValue("");
    await userEvent.click(screen.getByRole("button", { name: "关闭曲包详情" }));
    await openPack();
    expect(screen.getByRole("textbox", { name: "写评论" })).toHaveValue("My saved draft");
    expect(api.createBeatmapHubComment).not.toHaveBeenCalled();
  });
  it("reports comment mutation errors inside the detail and preserves the draft", async () => {
    connect(); api.createBeatmapHubComment.mockRejectedValue(new Error("Comment service unavailable"));
    renderPage(); await openPack();
    await userEvent.type(screen.getByRole("textbox", { name: "写评论" }), "Keep this comment");
    await userEvent.click(screen.getByRole("button", { name: "发送评论" }));
    const dialog = screen.getByRole("dialog");
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("Comment service unavailable");
    expect(screen.getByRole("textbox", { name: "写评论" })).toHaveValue("Keep this comment");
  });
  it("reuses downloaded archives when installation fails", async () => {
    let rejectInstall!: (error: Error) => void;
    api.getLocalSources.mockResolvedValue([{ client: "stable", valid: true, install_root: "C:\\osu" }]);
    api.downloadOnlineBeatmapsets.mockResolvedValue({ completed: 1, failed: 0, cancelled: false, completed_paths: ["map.osz"] });
    api.installCollectionDownloads.mockImplementationOnce(() => new Promise((_, reject) => { rejectInstall = reject; })).mockResolvedValue({ installed_sets: 1, resolved_entries: 1, unresolved_entries: 0 });
    api.getCollectionDownloadItems.mockResolvedValueOnce([{ beatmapset_id: 123, title: "Map", artist: "Artist" }]).mockResolvedValue([]);
    renderPage(); await openPack();
    await waitFor(() => expect(screen.getByRole("checkbox")).toBeChecked());
    await userEvent.dblClick(screen.getByRole("button", { name: "导入收藏夹" }));
    await waitFor(() => expect(api.installCollectionDownloads).toHaveBeenCalledTimes(1));
    await act(async () => rejectInstall(new Error("file locked")));
    await userEvent.click(await screen.findByRole("button", { name: "重试补齐" }));
    await screen.findByRole("button", { name: "已导入收藏夹" });
    expect(api.importBeatmapHubPack).toHaveBeenCalledTimes(1);
    expect(api.downloadOnlineBeatmapsets).toHaveBeenCalledTimes(1);
    expect(api.installCollectionDownloads).toHaveBeenCalledTimes(2);
  });
  it("updates community data without resolving metadata again", async () => {
    connect(); api.likeBeatmapHubPack.mockResolvedValue(undefined);
    renderPage(); await openPack();
    await userEvent.click(screen.getByRole("button", { name: "点赞 2" }));
    await waitFor(() => expect(api.likeBeatmapHubPack).toHaveBeenCalledWith("7K3N9A", true));
    await waitFor(() => expect(api.previewBeatmapHubPack.mock.calls.length).toBeGreaterThan(1));
    expect(api.getOnlineBeatmapset).toHaveBeenCalledTimes(1);
  });
  it("ignores a late preview response after another pack has been opened", async () => {
    let resolve!: (value: unknown) => void;
    const other = { ...pack, id: "ABCD23", title: "Second Pack", beatmapset_ids: [] };
    api.getBeatmapHubRecommendations.mockResolvedValue([pack, other]);
    api.previewBeatmapHubPack.mockImplementation((id: string) => id === pack.id ? new Promise((done) => { resolve = done; }) : Promise.resolve({ pack: other, missing_ids: [], locally_available_ids: [] }));
    renderPage();
    await showCommunity();
    await userEvent.click(await screen.findByRole("button", { name: "查看曲包 Tech Pack" }));
    await userEvent.click(screen.getByRole("button", { name: "关闭" }));
    await userEvent.click(screen.getByRole("button", { name: "查看曲包 Second Pack" }));
    await screen.findByRole("heading", { name: "Second Pack" });
    await act(async () => resolve(preview));
    expect(within(screen.getByRole("dialog")).queryByText("Tech Pack")).not.toBeInTheDocument();
  });
  const collectorCollection = { id: 24451, name: "jump", uploader: "Lesulite", uploader_id: 11426505, description: null, beatmap_count: 10, favourites: 3, last_modified: "2026-11-08T00:00:00Z", date_uploaded: "2026-11-08T00:00:00Z", difficulty_spread: [0, 0, 0, 1, 4, 6, 0, 0, 0, 0], modes: [{ mode: "osu", count: 10 }] };
  const collectorDetail = {
    collection: collectorCollection,
    total_sets: 3, local_sets: 1, missing_sets: 2, unsubmitted_beatmaps: 0,
    preview: [{ beatmap_id: 5192101, beatmapset_id: 2395248, checksum: "c2d40107e3fddd513a7726c0ad55c072", mode: "osu", difficulty_name: "FAITO'S HEART ATTACK", difficulty_rating: 6.49, artist: "Lil Texas", title: "Die Young", creator: "Tarrasky", local: false }],
    has_more: true,
  };
  it("browses osu!Collector collections and imports one as a local folder", async () => {
    api.getOsuCollectorCollections.mockResolvedValue({ collections: [collectorCollection], next_cursor: null, has_more: false, total: 1 });
    api.getOsuCollectorCollection.mockResolvedValue(collectorDetail);
    api.importOsuCollectorCollection.mockResolvedValue({ folder_id: "oc-folder", imported_sets: 3, imported_entries: 10, unresolved_sets: 0 });
    renderPage();
    const card = await screen.findByRole("button", { name: "查看收藏 jump" });
    // 无封面集合以难度分布直方图 + 模式/收藏数据为卡片主体。
    expect(screen.getByRole("img", { name: "难度分布直方图，共 10 张难度" })).toBeInTheDocument();
    expect(screen.getByText("共 1 个集合")).toBeInTheDocument();
    await userEvent.click(card);
    expect(await screen.findByRole("heading", { name: "jump" })).toBeInTheDocument();
    expect(screen.getByText("osu!Collector · Lesulite")).toBeInTheDocument();
    expect(screen.getByText("待下载")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "导入收藏夹" }));
    expect(await screen.findByRole("button", { name: "已导入收藏夹" })).toBeDisabled();
    expect(api.importOsuCollectorCollection).toHaveBeenCalledWith(24451);
    expect(api.downloadOnlineBeatmapsets).not.toHaveBeenCalled();
  });
  it("searches and sorts osu!Collector collections through the unified search bar", async () => {
    api.getOsuCollectorCollections.mockResolvedValue({ collections: [], next_cursor: null, has_more: false, total: 0 });
    renderPage();
    // 缺省“相关度”与官网一致：空关键词时该排序即按最新上传返回。
    await waitFor(() => expect(api.getOsuCollectorCollections).toHaveBeenLastCalledWith({ search: null, sortBy: "_text_match", orderBy: "desc", cursor: null }));
    await userEvent.type(screen.getByRole("textbox", { name: "搜索曲包或输入分享码" }), "nm1{Enter}");
    await waitFor(() => expect(api.getOsuCollectorCollections).toHaveBeenLastCalledWith({ search: "nm1", sortBy: "_text_match", orderBy: "desc", cursor: null }));
    await userEvent.click(screen.getByRole("button", { name: "收藏数" }));
    await waitFor(() => expect(api.getOsuCollectorCollections).toHaveBeenLastCalledWith({ search: "nm1", sortBy: "favourites", orderBy: "desc", cursor: null }));
    expect(await screen.findByText("没有匹配的收藏集合")).toBeInTheDocument();
  });
  it("shows an explicit network hint when osu!Collector is unreachable", async () => {
    api.getOsuCollectorCollections.mockRejectedValue({ code: "COLLECTOR_UNREACHABLE", message: "无法连接 osu!Collector：请检查网络连接（该站点在部分地区需要国际网络）后重试" });
    renderPage();
    expect(await screen.findByText("osu!Collector 暂时无法连接")).toBeInTheDocument();
    expect(screen.getByText(/该站点在部分地区需要国际网络/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "重新加载" })).toBeInTheDocument();
  });
  const romaiPool = { id: "6649b30ad5efb507e7b8d7b5", name: "Sample Cup 2026 (Finals)", elo: 1676, tier: "atomos", map_count: 2, counts: { nm: 1, hd: 0, hr: 0, dt: 0, fm: 0, tb: 1 }, covers: [1616036] };
  const romaiDetail = {
    reference: { provider: "romai", pool_id: romaiPool.id },
    title: romaiPool.name,
    tournament: "RomAI",
    entries: [
      { beatmap_id: 3299389, selection_type: "NM", position: 1, selected_by: null, selected_by_name: null, comment: "", is_custom: false, is_original: false, resolution_error: null,
        beatmap: { beatmapset_id: 1616036, title: "Sidetracked Day", artist: "TV Program", creator: "Sotarks", difficulty_name: "Extra", checksum: null, download_disabled: false, metadata: { difficulty_rating: 5.25 } } },
      { beatmap_id: 3299464, selection_type: "TB", position: 1, selected_by: null, selected_by_name: null, comment: "", is_custom: false, is_original: false, resolution_error: null, beatmap: null },
    ],
  };
  it("browses RomAI pools with tier filters and slot labels", async () => {
    api.getRomaiPoolSummaries.mockResolvedValue({ pools: [romaiPool], fetched_at: 1790000000, from_cache: true });
    api.getTournamentPool.mockResolvedValue(romaiDetail);
    renderPage();
    // 来源切换分段控件：三个来源常驻可见。
    expect(screen.getByRole("tab", { name: "社区曲包" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("tab", { name: "比赛图池" }));
    const card = await screen.findByRole("button", { name: "查看图池 Sample Cup 2026 (Finals)" });
    expect(await screen.findByText(/1 \/ 1 个图池 · 本地缓存/)).toBeInTheDocument();
    await userEvent.click(card);
    expect(await screen.findByRole("heading", { name: "Sample Cup 2026 (Finals)" })).toBeInTheDocument();
    // 图位以 NM1 / TB 槽位标识展示，未解析条目提示占位写入。
    expect(screen.getByText("NM1")).toBeInTheDocument();
    expect(screen.getAllByText("TB").length).toBeGreaterThan(1);
    expect(screen.getByText(/1 个图位的谱面资料暂时不可用/)).toBeInTheDocument();
  });
  it("syncs a RomAI pool into a labeled local collection folder", async () => {
    api.getRomaiPoolSummaries.mockResolvedValue({ pools: [romaiPool], fetched_at: 1790000000, from_cache: false });
    api.getTournamentPool.mockResolvedValue(romaiDetail);
    api.syncTournamentPoolCollection.mockResolvedValue({ folder_id: "romai-folder", entry_count: 2, pool: romaiDetail });
    renderPage();
    await userEvent.click(screen.getByRole("tab", { name: "比赛图池" }));
    await userEvent.click(await screen.findByRole("button", { name: "查看图池 Sample Cup 2026 (Finals)" }));
    await userEvent.click(await screen.findByRole("button", { name: "同步为收藏夹" }));
    expect(await screen.findByRole("button", { name: "再次同步" })).toBeInTheDocument();
    await waitFor(() => expect(api.syncTournamentPoolCollection).toHaveBeenCalledWith({ provider: "romai", pool_id: romaiPool.id }));
    expect(screen.getByRole("button", { name: "前往收藏夹" })).toBeInTheDocument();
  });
});
