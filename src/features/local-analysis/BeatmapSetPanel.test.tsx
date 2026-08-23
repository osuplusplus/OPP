import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { LocalBeatmapSetSummary, LocalBeatmapSummary } from "../../shared/types/osu";
import { useLocalBeatmapBackground, useLocalBeatmapSets } from "./api";
import { BeatmapSetPanel } from "./BeatmapSetPanel";

vi.mock("./api", () => ({
  useLocalBeatmapBackground: vi.fn(),
  useLocalBeatmapSets: vi.fn(),
}));

function difficulty(id: string, stars: number, name: string): LocalBeatmapSummary {
  return {
    resource: { resource_id: id, client: "stable", content_hash: `hash-${id}`, logical_path: `Songs/${id}` },
    set_key: "set-1",
    set_grouping_inferred: false,
    beatmap_id: Number(id),
    beatmap_set_id: 456,
    title: "Local Song",
    title_unicode: "",
    artist: "Local Artist",
    artist_unicode: "",
    creator: "Local Mapper",
    difficulty_name: name,
    ruleset: "osu",
    format_version: 14,
    stars,
    max_pp: 300,
    max_combo: 500,
    bpm: 180,
    length_ms: 120_000,
    object_count: 500,
    cs: 4,
    ar: 9,
    od: 8,
    hp: 6,
    average_nps: 4.2,
    peak_nps: 7.1,
    modified_at: null,
    analysis_status: "ready",
  };
}

const set: LocalBeatmapSetSummary = {
  set_key: "set-1",
  completeness: "complete",
  grouping_inferred: false,
  beatmap_set_id: 456,
  title: "Local Song",
  title_unicode: "",
  artist: "Local Artist",
  artist_unicode: "",
  creators: ["Local Mapper"],
  min_stars: 2.1,
  max_stars: 5.4,
  bpm: 180,
  length_ms: 120_000,
  object_count: 500,
  modified_at: null,
  background_resource_id: "501",
  difficulties: [difficulty("502", 5.4, "Insane"), difficulty("501", 2.1, "Easy")],
};

describe("BeatmapSetPanel", () => {
  beforeEach(() => {
    vi.mocked(useLocalBeatmapBackground).mockReturnValue({ data: "data:image/png;base64,cover" } as never);
    vi.mocked(useLocalBeatmapSets).mockReturnValue({
      data: { items: [set], total: 1, offset: 0, limit: 40 },
      error: null,
      isLoading: false,
      refetch: vi.fn(),
    } as never);
  });

  it("uses the online beatmap grid and media-card language for local sets", async () => {
    const user = userEvent.setup();
    const onOpen = vi.fn();
    const { container } = render(<BeatmapSetPanel client="stable" onOpen={onOpen} ruleset="osu" />);

    expect(container.querySelector(".opp-local-results")).toHaveClass("opp-online-results");
    expect(container.querySelector(".opp-local-results-grid")).toHaveClass("opp-online-results-grid");
    const card = screen.getByRole("button", { name: "查看本地谱面集 Local Song" });
    expect(card).toHaveClass("opp-media-card", "opp-beatmap-card", "aspect-[136/55]", "rounded-xl");
    expect(card.querySelector(".opp-beatmap-card__cover-overlay")).toBeInTheDocument();
    expect(card.querySelector(".opp-beatmap-card__details")).toBeInTheDocument();

    const compactRatings = Array.from(card.querySelectorAll('.opp-beatmap-card__difficulty-list [title$=" stars"]'), (node) => node.textContent?.trim());
    expect(compactRatings).toEqual(["2.10", "5.40"]);

    await user.click(card);
    expect(screen.getByRole("dialog")).toBeVisible();
    expect(screen.getByRole("heading", { name: "Local Song" })).toBeVisible();
  });
});
