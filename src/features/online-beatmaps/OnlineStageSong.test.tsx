import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { OnlineBeatmapset } from "../../shared/types/osu";
import { OnlineStageSong } from "./OnlineStageSong";

vi.mock("./useOnlineLocalPresence", () => ({ useOnlineLocalPresence: () => undefined }));

const set: OnlineBeatmapset = { id: 1, title: "Song", artist: "Artist", creator: "Mapper", status: "ranked", preview_url: "//preview.test/1.mp3", ranked_date: "2026-09-14T18:00:00Z", beatmaps: [
  { id: 10, beatmapset_id: 1, difficulty_rating: 2, total_length: 120, version: "Easy", mode: "osu", status: "ranked" },
  { id: 11, beatmapset_id: 1, difficulty_rating: 5, total_length: 120, version: "Insane", mode: "osu", status: "ranked" },
] };
const props = () => ({ set, ruleset: "osu" as const, playing: false, busy: false, loading: false, onPreview: vi.fn(), onDownload: vi.fn(), onDetails: vi.fn(), onCollect: vi.fn(), onVisualPreview: vi.fn(), onSimilar: vi.fn(), onWebsite: vi.fn(), onDifficultyWebsite: vi.fn(), onSearchTitle: vi.fn() });

describe("stage song controls", () => {
  it("tracks BID, searches titles, and keeps secondary actions behind More", async () => {
    const callbacks = props(); render(<OnlineStageSong {...callbacks} />); const user = userEvent.setup();
    expect(screen.getByText("BID 10")).toBeInTheDocument();
    expect(screen.getByText("上架 2026年9月15日")).toBeInTheDocument();
    expect(screen.getByText("上传 —")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "试听" })).toHaveTextContent("试听");
    await user.click(screen.getByRole("tab", { name: /Insane/ }));
    expect(screen.getByText("BID 11")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "打开当前难度官网" }));
    expect(callbacks.onDifficultyWebsite).toHaveBeenCalledWith(11, "osu");
    await user.click(screen.getByRole("button", { name: "搜索同名" })); expect(callbacks.onSearchTitle).toHaveBeenCalledOnce();
    expect(screen.queryByRole("button", { name: "完整详情" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "更多功能" }));
    await user.click(screen.getByRole("button", { name: "查找相似" })); expect(callbacks.onSimilar).toHaveBeenCalledWith(11, "osu");
    expect(screen.queryByRole("group", { name: "更多谱面功能" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "更多功能" }));
    await user.keyboard("{Escape}"); expect(screen.getByRole("button", { name: "更多功能" })).toHaveFocus();
    await user.click(screen.getByRole("button", { name: "更多功能" })); fireEvent.pointerDown(document.body);
    expect(screen.queryByRole("group", { name: "更多谱面功能" })).not.toBeInTheDocument();
  });
  it("handles missing difficulty and preview data", () => {
    render(<OnlineStageSong {...props()} set={{ ...set, beatmaps: [], preview_url: undefined }} />);
    expect(screen.getByText("BID —")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "打开当前难度官网" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "试听" })).toBeDisabled();
  });
});
