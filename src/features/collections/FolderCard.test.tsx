import { render, screen } from "@testing-library/react";
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

describe("FolderCard", () => {
  it("collapses to a compact header and expands its map grid again", async () => {
    const user = userEvent.setup();
    render(
      <FolderCard
        folder={folder}
        onChanged={vi.fn()}
        onDownload={vi.fn()}
      />,
    );

    expect(screen.getByText("Local Song")).toBeInTheDocument();
    const collapse = screen.getByRole("button", { name: "收起收藏夹 练习收藏" });
    expect(collapse).toHaveAttribute("aria-expanded", "true");

    await user.click(collapse);
    expect(screen.queryByText("Local Song")).not.toBeInTheDocument();
    expect(screen.getByText("Player · 1 个难度")).toBeInTheDocument();

    const expand = screen.getByRole("button", { name: "展开收藏夹 练习收藏" });
    expect(expand).toHaveAttribute("aria-expanded", "false");
    await user.click(expand);
    expect(screen.getByText("Local Song")).toBeInTheDocument();
  });
});
