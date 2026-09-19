import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { CollectionCandidate } from "../../shared/types/osu";
import { CollectionAddDialog } from "./CollectionAddDialog";
import { openCollectionDialog } from "./events";
import { desktopApi } from "../../shared/lib/tauri";

vi.mock("../../shared/lib/tauri", () => ({
  desktopApi: {
    listCollectionSummaries: vi.fn().mockResolvedValue({ folders: [], sources: [] }),
  },
}));

const candidates: CollectionCandidate[] = [
  {
    beatmap_id: 1,
    beatmapset_id: 10,
    checksum: null,
    ruleset: "osu",
    difficulty_name: "Hard",
    title: "Song",
    artist: "Artist",
    creator: "Mapper",
  },
  {
    beatmap_id: 2,
    beatmapset_id: 10,
    checksum: null,
    ruleset: "osu",
    difficulty_name: "Insane",
    title: "Song",
    artist: "Artist",
    creator: "Mapper",
  },
];

describe("CollectionAddDialog", () => {
  it("does not fetch the library while the global dialog is closed", async () => {
    vi.mocked(desktopApi.listCollectionSummaries).mockClear();
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(<QueryClientProvider client={queryClient}><CollectionAddDialog /></QueryClientProvider>);
    expect(desktopApi.listCollectionSummaries).not.toHaveBeenCalled();
    await act(async () => { openCollectionDialog(candidates); await import("./CollectionAddDialogContent"); });
    await waitFor(() => expect(desktopApi.listCollectionSummaries).toHaveBeenCalledTimes(1));
  });

  it("starts with every difficulty unselected", async () => {
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    render(
      <QueryClientProvider client={queryClient}>
        <CollectionAddDialog />
      </QueryClientProvider>,
    );

    await act(async () => { openCollectionDialog(candidates); await import("./CollectionAddDialogContent"); });

    await waitFor(() => expect(screen.getByRole("dialog")).toBeInTheDocument());
    expect(screen.getAllByRole("checkbox")).toHaveLength(2);
    for (const checkbox of screen.getAllByRole("checkbox")) {
      expect(checkbox).not.toBeChecked();
    }
    expect(screen.getByRole("button", { name: "加入收藏夹" })).toBeDisabled();
  });
});
