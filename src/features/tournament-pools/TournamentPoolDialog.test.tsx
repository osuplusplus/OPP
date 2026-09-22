import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { desktopApi } from "../../shared/lib/tauri";
import type { TournamentPool, TournamentPoolRef } from "../../shared/types/osu";
import TournamentPoolDialog from "./TournamentPoolDialog";

const download = vi.hoisted(() => ({ start: vi.fn(), cancel: vi.fn(), saveDestination: vi.fn(), destination: "C:/Maps", defaultProvider: "sayobot",
  state: { busy: false, progress: null, result: null, error: null } as { busy: boolean; progress: null; result: null; error: string | null },
}));
vi.mock("../online-beatmaps/api", () => ({ useBeatmapDownloads: () => download }));
vi.mock("../../shared/lib/tauri", async (original) => {
  const real = await original<typeof import("../../shared/lib/tauri")>();
  return { ...real, desktopApi: { ...real.desktopApi, getTournamentPool: vi.fn(), syncTournamentPoolCollection: vi.fn() } };
});

const reference: TournamentPoolRef = { provider: "rino", season: "s1", category: "qualification" };
const pool: TournamentPool = { reference, title: "Rino S1 资格赛", entries: [
  { beatmap_id: 10, selection_type: "LZ", position: 1, selected_by: "123", selected_by_name: null,
    comment: "<script>alert(1)</script>", is_custom: true, is_original: true, resolution_error: null,
    beatmap: { beatmapset_id: 20, title: "Song", artist: "Artist", creator: "Mapper", difficulty_name: "Expert", checksum: null, download_disabled: false } },
  { beatmap_id: 11, selection_type: "TB", position: 1, selected_by: null, selected_by_name: null,
    comment: "", is_custom: false, is_original: false, beatmap: null, resolution_error: "查询失败" },
] };
const clients: QueryClient[] = [];
function mount() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  clients.push(client);
  render(<QueryClientProvider client={client}><MemoryRouter><TournamentPoolDialog reference={reference} onClose={vi.fn()} /></MemoryRouter></QueryClientProvider>);
  return client;
}
beforeEach(() => {
  vi.clearAllMocks();
  download.state.busy = false; download.state.error = null; download.defaultProvider = "sayobot";
  vi.mocked(desktopApi.getTournamentPool).mockResolvedValue(pool);
  vi.mocked(desktopApi.syncTournamentPoolCollection).mockResolvedValue({ folder_id: "linked", entry_count: 2, pool });
});
afterEach(() => { cleanup(); clients.splice(0).forEach((client) => client.clear()); });

it("opens a read-only preview, escapes comments and leaves partial failures actionable", async () => {
  mount();
  expect(await screen.findByText("LZ1")).toBeInTheDocument();
  expect(screen.getByText("TB1")).toBeInTheDocument();
  expect(screen.getByText("<script>alert(1)</script>")).toBeInTheDocument();
  expect(document.querySelector("script")).toBeNull();
  expect(screen.getByText("比赛定制 · 原创")).toBeInTheDocument();
  expect(screen.getByText(/将跳过：BID 11/)).toBeInTheDocument();
  expect(desktopApi.syncTournamentPoolCollection).not.toHaveBeenCalled();
  expect(download.start).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole("button", { name: "下载图池 · 1 个曲包" }));
  expect(download.start).toHaveBeenCalledWith([expect.objectContaining({ id: 20, beatmaps: [{ id: 10 }] })], { provider: "sayobot" });
  expect(desktopApi.syncTournamentPoolCollection).not.toHaveBeenCalled();
});

it("syncs only after a click and refreshes the linked collection caches", async () => {
  const client = mount();
  const invalidation = vi.spyOn(client, "invalidateQueries");
  await screen.findByText("LZ1");
  expect(screen.getByText(/同步将替换本阶段收藏夹/)).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "同步到收藏" }));
  expect(await screen.findByText("已同步 2 个难度到收藏夹。")).toBeInTheDocument();
  expect(desktopApi.syncTournamentPoolCollection).toHaveBeenCalledExactlyOnceWith(reference);
  expect(invalidation).toHaveBeenCalledWith({ queryKey: ["collections"] });
  expect(invalidation).toHaveBeenCalledWith({ queryKey: ["collection-entries", "linked"] });
  expect(download.start).not.toHaveBeenCalled();
});

it("handles empty pools and failures without enabling destructive actions", async () => {
  vi.mocked(desktopApi.getTournamentPool).mockResolvedValue({ ...pool, entries: [] });
  mount();
  await screen.findByText("本阶段暂未发布图池");
  expect(screen.getByRole("button", { name: "同步到收藏" })).toBeDisabled();
  expect(screen.getByRole("button", { name: /下载图池/ })).toBeDisabled();
  vi.mocked(desktopApi.getTournamentPool).mockRejectedValue({ message: "接口离线" });
  await userEvent.click(screen.getByRole("button", { name: "刷新" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("接口离线");
  expect(screen.getByRole("button", { name: "同步到收藏" })).toBeDisabled();
});

it("disables another download while busy and exposes cancellation and errors", async () => {
  download.state.busy = true;
  download.state.error = "另一个下载任务正在运行";
  mount();
  await screen.findByText("LZ1");
  expect(screen.getByRole("button", { name: /下载图池/ })).toBeDisabled();
  expect(screen.getByRole("alert")).toHaveTextContent("另一个下载任务正在运行");
  await userEvent.click(screen.getByRole("button", { name: "取消当前下载" }));
  expect(download.cancel).toHaveBeenCalledOnce();
  expect(download.start).not.toHaveBeenCalled();
});

it("requires a chosen provider and presents collection failures", async () => {
  download.defaultProvider = "none";
  vi.mocked(desktopApi.syncTournamentPoolCollection).mockRejectedValue({ message: "图池已撤回" });
  mount();
  await screen.findByText("LZ1");
  expect(screen.getByRole("button", { name: /下载图池/ })).toBeDisabled();
  await userEvent.click(screen.getByText(/下载设置/));
  await userEvent.selectOptions(screen.getByLabelText("比赛图池下载源"), "nerinyan");
  await waitFor(() => expect(screen.getByRole("button", { name: /下载图池/ })).toBeEnabled());
  await userEvent.click(screen.getByRole("button", { name: "同步到收藏" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("图池已撤回");
});
