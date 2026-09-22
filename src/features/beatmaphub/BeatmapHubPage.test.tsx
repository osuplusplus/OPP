import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { BeatmapHubPack } from "../../shared/types/osu";

const api = vi.hoisted(() => Object.fromEntries([
  "getBeatmapHubAuthStatus", "getAuthStatus", "listCollectionSummaries", "getBeatmapHubProfile", "previewBeatmapHubPack", "getOnlineBeatmapset", "getBeatmapHubRecommendations", "searchBeatmapHubPacks", "getBeatmapHubComments", "getSettings", "getLocalSources", "onBeatmapDownloadProgress", "createBeatmapHubProfile", "linkBeatmapHubDevice", "loginBeatmapHub", "logoutBeatmapHub", "createBeatmapHubDeviceLink", "revokeBeatmapHubDevice", "publishBeatmapHubPack", "updateBeatmapHubPack", "deleteBeatmapHubPack", "favoriteBeatmapHubPack", "likeBeatmapHubPack", "rateBeatmapHubPack", "createBeatmapHubComment", "updateBeatmapHubComment", "deleteBeatmapHubComment", "importCollectionArchive", "importBeatmapHubPack", "beginCollectionTask", "getCollectionDownloadItems", "downloadOnlineBeatmapsets", "installCollectionDownloads",
].map((name) => [name, vi.fn()])));
vi.mock("../../shared/lib/tauri", () => ({ desktopApi: api, isTauri: () => true }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
import { BeatmapHubPage } from "./BeatmapHubPage";

const pack: BeatmapHubPack = { id: "7K3N9A", title: "Tech Pack", description: "Practice", is_private: false, owner: { id: "user", display_name: "Player" }, beatmapset_ids: [123], manifest_hash: "hash", rating: { average: 4.5, count: 2 }, likes: { count: 2 }, comments: { count: 0 }, viewer: null, created_at: "2026-01-01", updated_at: "2026-01-02" };
const preview = { pack, locally_available_ids: [], missing_ids: [123] };
function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  return { ...render(<QueryClientProvider client={client}><MemoryRouter><BeatmapHubPage /></MemoryRouter></QueryClientProvider>), client };
}
async function openPack() {
  await userEvent.click(await screen.findByRole("button", { name: "查看曲包 Tech Pack" }));
  await screen.findByRole("heading", { name: "Tech Pack" });
  await waitFor(() => expect(screen.getByRole("button", { name: "导入收藏夹" })).toBeEnabled());
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
  });

  it("shows discovery for guests without parsing packs or showing identity setup", async () => {
    renderPage();
    expect(await screen.findByRole("button", { name: "查看曲包 Tech Pack" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "连接 BeatmapHub" })).not.toBeInTheDocument();
    expect(api.getOnlineBeatmapset).not.toHaveBeenCalled();
  });
  it("opens identity management on demand with default names and device linking", async () => {
    renderPage();
    await userEvent.click(screen.getByRole("button", { name: "连接身份" }));
    expect(await screen.findByDisplayValue("L1rics")).toBeInTheDocument();
    expect(screen.getByDisplayValue("TEST-PC")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "链接已有档案" }));
    expect(screen.getByPlaceholderText("粘贴旧设备生成的 43 位链接码")).toBeInTheDocument();
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
    api.createBeatmapHubProfile.mockImplementation(async () => { connect(); });
    renderPage();
    await userEvent.click(screen.getByRole("button", { name: "发布曲包" }));
    await userEvent.selectOptions(await screen.findByRole("combobox"), "folder");
    await userEvent.type(screen.getByRole("textbox", { name: /说明/ }), "Keep my draft");
    await userEvent.click(screen.getByRole("button", { name: "连接身份后继续" }));
    await userEvent.click(await screen.findByRole("button", { name: "创建并连接" }));
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
    renderPage(); await userEvent.click(await screen.findByRole("button", { name: "查看曲包 Tech Pack" }));
    await userEvent.click(screen.getByRole("button", { name: "关闭" }));
    await userEvent.click(screen.getByRole("button", { name: "查看曲包 Second Pack" }));
    await screen.findByRole("heading", { name: "Second Pack" });
    await act(async () => resolve(preview));
    expect(within(screen.getByRole("dialog")).queryByText("Tech Pack")).not.toBeInTheDocument();
  });
});
