import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { desktopApi } from "../../shared/lib/tauri";
import type { CollectionPoolSnapshot } from "../../shared/types/osu";
import { PoolMetadataRepair } from "./PoolMetadataRepair";

vi.mock("../../shared/lib/tauri", () => ({ desktopApi: { repairTournamentPoolMetadata: vi.fn() } }));
const pool: CollectionPoolSnapshot = { reference: { provider: "opp", url: "https://example.com/pool.json" }, slots: [{ beatmap_id: 10, label: "NM1", comment: "", selected_by: "" }] };
beforeEach(() => vi.resetAllMocks());
it("backfills old snapshots, refreshes queries and offers retry for partial metadata failures", async () => {
  vi.mocked(desktopApi.repairTournamentPoolMetadata).mockResolvedValueOnce([1, 1]).mockResolvedValueOnce([1, 0]);
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: 0 } } });
  const invalidate = vi.spyOn(client, "invalidateQueries");
  const view = render(<QueryClientProvider client={client}><PoolMetadataRepair folderId="folder" pool={pool} /></QueryClientProvider>);
  await userEvent.click(await screen.findByRole("button", { name: /点击重试/ }));
  await waitFor(() => expect(screen.queryByRole("button")).not.toBeInTheDocument());
  expect(desktopApi.repairTournamentPoolMetadata).toHaveBeenCalledTimes(2);
  expect(invalidate).toHaveBeenCalledWith({ queryKey: ["collections"] });
  expect(invalidate).toHaveBeenCalledWith({ queryKey: ["collection-entries", "folder"] });
  view.unmount(); client.clear();
});
it("does not fetch metadata again for a complete saved snapshot", () => {
  const client = new QueryClient();
  const metadata = { id: 10, beatmapset_id: 1, mode: "osu" as const, status: "ranked", version: "Hard", difficulty_rating: 5, total_length: 120 };
  const view = render(<QueryClientProvider client={client}><PoolMetadataRepair folderId="folder" pool={{ ...pool, slots: [{ ...pool.slots[0], metadata }] }} /></QueryClientProvider>);
  expect(desktopApi.repairTournamentPoolMetadata).not.toHaveBeenCalled();
  view.unmount(); client.clear();
});
