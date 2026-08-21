import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import { desktopApi } from "../../shared/lib/tauri";
import type { OnlineBeatmapset } from "../../shared/types/osu";
import { BeatmapsetDetailDialog } from "./BeatmapsetDetailDialog";

const mixedBeatmapset: OnlineBeatmapset = {
  id: 10,
  artist: "Mixed Artist",
  title: "Mixed Set",
  creator: "Mapper",
  status: "ranked",
  beatmaps: [
    { id: 101, beatmapset_id: 10, difficulty_rating: 4, mode: "osu", status: "ranked", total_length: 120, version: "Standard" },
    { id: 102, beatmapset_id: 10, difficulty_rating: 5, mode: "mania", status: "ranked", total_length: 120, version: "Mania 4K" },
  ],
};

afterEach(() => vi.restoreAllMocks());

describe("BeatmapsetDetailDialog", () => {
  it("uses the selected difficulty ruleset for similarity deep links in mixed sets", async () => {
    const user = userEvent.setup();
    vi.spyOn(desktopApi, "getOnlineBeatmapset").mockResolvedValue(mixedBeatmapset);
    const onFindSimilar = vi.fn();
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={client}>
        <BeatmapsetDetailDialog
          beatmapsetId={10}
          fallback={mixedBeatmapset}
          onAddToCollection={vi.fn()}
          onClose={vi.fn()}
          onFindSimilar={onFindSimilar}
          onPreview={vi.fn()}
          playing={false}
        />
      </QueryClientProvider>,
    );

    const buttons = await screen.findAllByRole("button", { name: "查找相似" });
    await user.click(buttons[0]);
    await user.click(buttons[1]);
    expect(onFindSimilar).toHaveBeenNthCalledWith(1, 101, "osu");
    expect(onFindSimilar).toHaveBeenNthCalledWith(2, 102, "mania");
  });
});
