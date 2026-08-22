import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { CollectionEntry } from "../../shared/types/osu";
import { MapCard } from "./MapCard";

const entry: CollectionEntry = {
  id: "entry-1",
  beatmap_id: 123,
  beatmapset_id: 697087,
  checksum: "abc123",
  ruleset: "osu",
  difficulty_name: "Insane",
  title: "Daisuke",
  artist: "Y&Co.",
  creator: "moph",
  resolved: true,
};

describe("MapCard", () => {
  it("renders one concrete map instead of a beatmapset summary", () => {
    render(<MapCard busy={false} entry={entry} onRemove={vi.fn()} readOnly={false} />);

    expect(screen.getByText("Daisuke")).toBeInTheDocument();
    expect(screen.getByText("Insane")).toBeInTheDocument();
    expect(screen.getByText("osu!")).toBeInTheDocument();
    expect(screen.getByText("已在本地")).toBeInTheDocument();
    expect(screen.getByText("#123")).toBeInTheDocument();
  });

  it("keeps removal available only for writable collections", async () => {
    const user = userEvent.setup();
    const onRemove = vi.fn();
    const { rerender } = render(<MapCard busy={false} entry={entry} onRemove={onRemove} readOnly={false} />);

    await user.click(screen.getByRole("button", { name: "从收藏夹移除 Daisuke Insane" }));
    expect(onRemove).toHaveBeenCalledOnce();

    rerender(<MapCard busy={false} entry={entry} onRemove={onRemove} readOnly />);
    expect(screen.queryByRole("button", { name: "从收藏夹移除 Daisuke Insane" })).not.toBeInTheDocument();
  });
});
