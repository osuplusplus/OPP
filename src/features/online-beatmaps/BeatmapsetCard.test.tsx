import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { OnlineBeatmapset } from "../../shared/types/osu";
import { BeatmapsetCard } from "./BeatmapsetCard";

const beatmapset: OnlineBeatmapset = {
  id: 697087,
  artist: "Y&Co.",
  title: "Daisuke",
  creator: "moph",
  status: "ranked",
  covers: { card: "https://example.com/cover.jpg" },
  beatmaps: [{
    id: 123,
    beatmapset_id: 697087,
    difficulty_rating: 4.52,
    mode: "osu",
    status: "ranked",
    total_length: 128,
    version: "Insane",
  }],
};

describe("BeatmapsetCard", () => {
  it("keeps queue selection separate from direct download", async () => {
    const user = userEvent.setup();
    const onDownload = vi.fn();
    const onOpen = vi.fn();
    const onSelect = vi.fn();

    render(
      <BeatmapsetCard
        beatmapset={beatmapset}
        downloading={false}
        onDownload={onDownload}
        onOpen={onOpen}
        onPreview={vi.fn()}
        onSelect={onSelect}
        playing={false}
        selected={false}
      />,
    );

    await user.click(screen.getByRole("button", { name: "下载谱面" }));
    expect(onDownload).toHaveBeenCalledOnce();
    expect(onOpen).not.toHaveBeenCalled();

    await user.click(screen.getByRole("button", { name: "加入下载队列" }));
    expect(onSelect).toHaveBeenCalledOnce();
    expect(onOpen).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "试听" })).toHaveClass("opp-beatmap-card__wide-action");
    expect(screen.getByRole("button", { name: "预览详情" })).toHaveClass("opp-beatmap-card__wide-action");
  });

  it("shows the compact osu metadata and opens from the keyboard", async () => {
    const user = userEvent.setup();
    const onOpen = vi.fn();

    render(
      <BeatmapsetCard
        beatmapset={beatmapset}
        downloading={false}
        onDownload={vi.fn()}
        onOpen={onOpen}
        onPreview={vi.fn()}
        onSelect={vi.fn()}
        playing={false}
        selected={false}
      />,
    );

    expect(screen.getByText(/谱师 ·/)).toHaveTextContent("谱师 · moph");
    expect(screen.getAllByText("Insane", { selector: "span" })).not.toHaveLength(0);
    expect(screen.getAllByText("上架")).not.toHaveLength(0);

    const card = screen.getByRole("button", { name: "查看谱面 Daisuke" });
    expect(card).toHaveClass("opp-media-card", "aspect-[136/55]", "min-h-40", "rounded-xl");
    expect(card.querySelector(".opp-media-card__cover")).toBeInTheDocument();
    expect(card.querySelector(".opp-beatmap-card__cover-overlay")).toBeInTheDocument();
    expect(card.querySelector(".opp-beatmap-card__details")).toBeInTheDocument();
    expect(card).not.toHaveClass("h-[220px]");
    card.focus();
    await user.keyboard("{Enter}");
    expect(onOpen).toHaveBeenCalledOnce();
  });
});
