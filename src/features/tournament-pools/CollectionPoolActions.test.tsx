import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { desktopApi } from "../../shared/lib/tauri";
import type { CollectionEntry, CollectionFolderSummary, CollectionPoolSnapshot } from "../../shared/types/osu";
import { CollectionPoolActions } from "./CollectionPoolActions";

const downloads = vi.hoisted(() => ({
  state: { busy: false, progress: null, result: null, error: null }, start: vi.fn().mockResolvedValue(undefined),
  cancel: vi.fn(), destination: "", saveDestination: vi.fn(), defaultProvider: "nerinyan",
}));
vi.mock("../online-beatmaps/api", () => ({ useBeatmapDownloads: () => downloads }));
vi.mock("../../shared/lib/tauri", async (original) => {
  const real = await original<typeof import("../../shared/lib/tauri")>();
  return { ...real, desktopApi: { ...real.desktopApi, queryCollectionEntries: vi.fn(), syncTournamentPoolCollection: vi.fn(), writeStableCollections: vi.fn(), openCollectionDownloads: vi.fn() } };
});
const folder: CollectionFolderSummary = { id: "pool", name: "Pool", creator: "user", source: "opp", stable_sync: false, read_only: false, pending_write: false, entry_count: 3, missing_count: 2, beatmapset_count: 1, revision: 3 };
const pool: CollectionPoolSnapshot = { title: "资格赛", tournament: "ASC 星域杯", season: "S1", reference: { provider: "opp", url: "https://example.com/pool.json" }, slots: [] };
const entry = (id: number, set: number | null): CollectionEntry => ({ id: `${id}`, beatmap_id: id, beatmapset_id: set, checksum: null, ruleset: "osu", title: "Song", artist: "Artist", creator: "Mapper", difficulty_name: "Hard", resolved: false });
beforeEach(() => {
  vi.clearAllMocks(); downloads.state.busy = false;
  vi.mocked(desktopApi.queryCollectionEntries).mockResolvedValue({ items: [entry(1, 10), entry(2, 10), entry(3, null)], total: 3, offset: 0, limit: 100, revision: 3 });
});
function setup() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: 0 } } });
  const onImportStable = vi.fn().mockResolvedValue(undefined);
  render(<QueryClientProvider client={client}><CollectionPoolActions folder={folder} pool={pool} onImportStable={onImportStable} /></QueryClientProvider>);
  return { client, onImportStable };
}
it("downloads stored entries with exact difficulties and never writes or opens game files", async () => {
  const { client, onImportStable } = setup();
  expect(screen.getByText("仅保存在 OPP · osu!standard")).toBeInTheDocument();
  expect(onImportStable).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole("button", { name: "下载图池" }));
  await waitFor(() => expect(downloads.start).toHaveBeenCalledWith([
    expect.objectContaining({ id: 10, beatmaps: [{ id: 1 }, { id: 2 }], allow_extra_difficulties: true }),
  ], { provider: "nerinyan", openAfterDownload: false }));
  expect(screen.getByRole("status")).toHaveTextContent("BID 3");
  expect(desktopApi.syncTournamentPoolCollection).not.toHaveBeenCalled();
  expect(desktopApi.writeStableCollections).not.toHaveBeenCalled(); expect(desktopApi.openCollectionDownloads).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole("button", { name: "导入 Stable" }));
  expect(onImportStable).toHaveBeenCalledOnce(); client.clear();
});
it("syncs only on explicit request and invalidates summaries, browser and entry queries", async () => {
  vi.mocked(desktopApi.syncTournamentPoolCollection).mockResolvedValue({ folder_id: "pool", entry_count: 2, pool: { reference: pool.reference, title: "Updated", entries: [] } });
  const { client } = setup(); const invalidate = vi.spyOn(client, "invalidateQueries");
  expect(desktopApi.syncTournamentPoolCollection).not.toHaveBeenCalled();
  await userEvent.click(screen.getByText("图池设置"));
  await userEvent.click(screen.getByRole("button", { name: "同步图池" }));
  await waitFor(() => expect(desktopApi.syncTournamentPoolCollection).toHaveBeenCalledOnce());
  expect(invalidate).toHaveBeenCalledWith({ queryKey: ["collections"] });
  expect(invalidate).toHaveBeenCalledWith({ queryKey: ["collection-entries", "pool"] }); client.clear();
});
it("disables starting another download while the shared session is active", () => {
  downloads.state.busy = true; const { client } = setup();
  expect(screen.getByRole("button", { name: "下载图池" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "取消当前下载" })).toBeInTheDocument(); client.clear();
});
