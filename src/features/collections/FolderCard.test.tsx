import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { desktopApi } from "../../shared/lib/tauri";
import type { ReactElement } from "react";
import { render as renderBase, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { CollectionFolder } from "../../shared/types/osu";
import { FolderCard } from "./CollectionsPage";

const folder: CollectionFolder = {
  id: "folder-1",
  name: "练习收藏",
  creator: "Player",
  created_at: "2026-08-23T00:00:00Z",
  updated_at: "2026-08-23T00:00:00Z",
  source: "stable",
  read_only: false,
  pending_write: false,
  entries: [{
    id: "entry-1",
    beatmap_id: 123,
    beatmapset_id: 456,
    checksum: "checksum",
    ruleset: "osu",
    difficulty_name: "Insane",
    title: "Local Song",
    artist: "Local Artist",
    creator: "Local Mapper",
    resolved: true,
  }],
};

vi.mock("../../shared/lib/tauri", async (original) => {
  const real = await original<typeof import("../../shared/lib/tauri")>();
  return { ...real, desktopApi: { ...real.desktopApi, queryCollectionEntries: vi.fn() } };
});
function summary(value: CollectionFolder) {
  vi.mocked(desktopApi.queryCollectionEntries).mockImplementation(async (_id, offset, limit, revision) => ({ items: value.entries.slice(offset, offset + limit), offset, limit, total: value.entries.length, revision }));
  return { ...value, entry_count: value.entries.length, missing_count: 0, beatmapset_count: 1, revision: 1 };
}
function render(element: ReactElement) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return renderBase(<QueryClientProvider client={client}>{element}</QueryClientProvider>);
}
describe("FolderCard", () => {
  it("bounds mounted cards for a large collection and keeps every entry reachable", async () => {
    const user = userEvent.setup();
    const largeFolder = { ...folder, entries: Array.from({ length: 2000 }, (_, index) => ({
      ...folder.entries[0], id: `entry-${index}`, title: `Song ${index}`,
    })) };
    const { container } = render(<FolderCard folder={summary(largeFolder)} onChanged={vi.fn()} onDownload={vi.fn()} />);
    await screen.findByText("Song 0");
    expect(container.querySelectorAll(".opp-map-card")).toHaveLength(12);
    expect(screen.getByText("Song 0")).toBeInTheDocument();
    expect(screen.queryByText("Song 12")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "练习收藏下一页" }));
    await screen.findByText("Song 12");
    expect(screen.queryByText("Song 0")).not.toBeInTheDocument();
    expect(screen.getByText("Song 12")).toBeInTheDocument();
    expect(container.querySelectorAll(".opp-map-card")).toHaveLength(12);
  });

  it("collapses to a compact header and expands its map grid again", async () => {
    const user = userEvent.setup();
    render(
      <FolderCard
        folder={summary(folder)}
        onChanged={vi.fn()}
        onDownload={vi.fn()}
      />,
    );

    expect(await screen.findByText("Local Song")).toBeInTheDocument();
    const collapse = screen.getByRole("button", { name: "收起收藏夹 练习收藏" });
    expect(collapse).toHaveAttribute("aria-expanded", "true");

    await user.click(collapse);
    expect(screen.queryByText("Local Song")).not.toBeInTheDocument();
    expect(screen.getByText("Player · 1 个难度")).toBeInTheDocument();

    const expand = screen.getByRole("button", { name: "展开收藏夹 练习收藏" });
    expect(expand).toHaveAttribute("aria-expanded", "false");
    await user.click(expand);
    expect(await screen.findByText("Local Song")).toBeInTheDocument();
  });
});
