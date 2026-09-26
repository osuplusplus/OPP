import { describe, expect, it } from "vitest";

import type { BeatmapDownloadProgress } from "../../shared/types/osu";
import { createDownloadProgressDisplay } from "./downloadProgressDisplay";

function progress(overrides: Partial<BeatmapDownloadProgress> = {}): BeatmapDownloadProgress {
  return {
    phase: "downloading",
    total: 3,
    processed: 0,
    completed: 0,
    skipped: 0,
    failed: 0,
    current_beatmapset_id: 1,
    current_title: "Artist — First",
    message: "正在下载曲包",
    downloaded_bytes: 1,
    total_bytes: 10,
    bytes_per_second: 2,
    ...overrides,
  };
}

describe("download progress display", () => {
  it("keeps the first active title while concurrent workers report progress", () => {
    const display = createDownloadProgressDisplay();

    expect(display.update(progress())).toMatchObject({ current_title: "Artist — First" });
    expect(display.update(progress({ current_beatmapset_id: 2, current_title: "Artist — Second", downloaded_bytes: 6 }))).toMatchObject({
      current_title: "Artist — First",
      current_beatmapset_id: 1,
      downloaded_bytes: 1,
      processed: 0,
    });
  });

  it("moves to the next worker after the visible item finishes", () => {
    const display = createDownloadProgressDisplay();
    display.update(progress());
    display.update(progress({ current_beatmapset_id: 2, current_title: "Artist — Second" }));

    const next = display.update(progress({
      phase: "completed",
      current_title: "Artist — First",
      processed: 1,
      completed: 1,
    }));
    expect(next).toMatchObject({ current_beatmapset_id: 2, current_title: "Artist — Second", phase: "completed" });
  });

  it("keeps aggregate counters from moving backwards when worker events race", () => {
    const display = createDownloadProgressDisplay();
    display.update(progress({ processed: 5, completed: 5 }));
    expect(display.update(progress({ processed: 4, completed: 4 }))).toMatchObject({ processed: 5, completed: 5 });
  });

  it("clears item state when a batch starts or ends", () => {
    const display = createDownloadProgressDisplay();
    display.update(progress());
    display.update(progress({ phase: "finished", current_beatmapset_id: null, current_title: null }));

    expect(display.update(progress({ current_beatmapset_id: 2, current_title: "Artist — New batch" }))).toMatchObject({
      current_beatmapset_id: 2,
      current_title: "Artist — New batch",
    });
  });
});
