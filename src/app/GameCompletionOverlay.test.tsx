import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import { desktopApi } from "../shared/lib/tauri";
import type { AppSettings, DanserRenderPreferences, GameSessionSummary, NewReplaysDetected } from "../shared/types/osu";
import { GameCompletionOverlay } from "./AppShell";

const preferences: DanserRenderPreferences = {
  settings_profile: "default", skin: "", skip: true, quickstart: false,
  start: null, end: null, speed: 1, pitch: 1, offset: 0, mods: "", mods2: "",
  cs: null, ar: null, od: null, hp: null, no_db_check: true,
  no_update_check: true, debug: false, settings_patch: "",
  frame_width: 1920, frame_height: 1080, fps: 60, encoder: "libx264",
  quality: 14, motion_blur: false, motion_blur_oversample: 16,
};

const discovery: NewReplaysDetected = {
  client: "stable",
  started_at: "2026-08-13T10:00:00Z",
  detected_at: "2026-08-13T10:05:00Z",
  replays: [{
    path: "C:\\osu!\\Replays\\new.osr",
    file_name: "new.osr",
    beatmap_title: "Artist — Song [Insane]",
    username: "Player",
    renderable: true,
    reason: null,
  }],
};

afterEach(() => vi.restoreAllMocks());

describe("GameCompletionOverlay", () => {
  it("shows the requested session score statistics and their changes", () => {
    vi.spyOn(desktopApi, "getDanserStatus").mockRejectedValue(new Error("not configured"));
    const snapshot = {
      captured_at: "2026-08-13T10:00:00Z", username: "Player",
      pp: 100, ranked_score: 1_000, hit_accuracy: 98,
      total_hits: 500, total_score: 2_000,
    };
    const session: GameSessionSummary = {
      started_at: snapshot.captured_at, ended_at: "2026-08-13T10:05:00Z",
      ruleset: "osu", client: "stable", executable: "C:\\osu!\\osu!.exe",
      start: snapshot,
      end: {
        ...snapshot, captured_at: "2026-08-13T10:05:00Z", pp: 101.25,
        ranked_score: 1_250, hit_accuracy: 98.5, total_hits: 560, total_score: 2_900,
      },
      running: false,
    };

    render(<GameCompletionOverlay discovery={null} onClose={vi.fn()} onNavigate={vi.fn()} session={session} settings={undefined} />);

    for (const label of ["PP", "计分成绩", "准确率", "总命中次数", "总分"]) {
      expect(screen.getByText(label)).toBeInTheDocument();
    }
    expect(screen.getByText("1,250 (+250)")).toBeInTheDocument();
    expect(screen.getByText("2,900 (+900)")).toBeInTheDocument();
    expect(screen.queryByText("BP 最高 PP")).not.toBeInTheDocument();
    expect(screen.queryByText("最大连击")).not.toBeInTheDocument();
  });

  it("compacts large session totals instead of listing every digit", () => {
    vi.spyOn(desktopApi, "getDanserStatus").mockRejectedValue(new Error("not configured"));
    const start = {
      captured_at: "2026-08-13T10:00:00Z", username: "Player",
      pp: 7501.66, ranked_score: 89_581_114_458, hit_accuracy: 98.49,
      total_hits: 19_122_026, total_score: 194_509_296_422,
    };
    const session: GameSessionSummary = {
      started_at: start.captured_at, ended_at: "2026-08-13T10:05:00Z",
      ruleset: "osu", client: "stable", executable: "C:\\osu!\\osu!.exe",
      start: { ...start, ranked_score: 89_578_000_000, total_hits: 19_121_780, total_score: 194_506_160_000 },
      end: { ...start, captured_at: "2026-08-13T10:05:00Z" },
      running: false,
    };

    render(<GameCompletionOverlay discovery={null} onClose={vi.fn()} onNavigate={vi.fn()} session={session} settings={undefined} />);

    expect(screen.getByText(/895\.81亿/)).toBeInTheDocument();
    expect(screen.queryByText(/89,581,114,458/)).not.toBeInTheDocument();
  });

  it("allows a replay to be selected and queued without starting rendering", async () => {
    const user = userEvent.setup();
    vi.spyOn(desktopApi, "getDanserStatus").mockResolvedValue({
      available: true,
      executable_path: "C:\\danser\\danser-cli.exe",
      ffmpeg_available: true,
      profiles: ["default"],
      message: "ready",
    });
    const enqueue = vi.spyOn(desktopApi, "enqueueDanserRenders").mockResolvedValue([]);
    const start = vi.spyOn(desktopApi, "startDanserRenderQueue").mockResolvedValue(undefined);
    const onClose = vi.fn();
    const onNavigate = vi.fn();

    render(<GameCompletionOverlay
      discovery={discovery}
      onClose={onClose}
      onNavigate={onNavigate}
      session={null}
      settings={{
        auto_export_new_replays_with_danser: false,
        replay_export_directory: "D:\\renders",
        danser_render_preferences: preferences,
      } as AppSettings}
    />);

    const checkbox = screen.getByRole("checkbox", { name: "选择 Artist — Song [Insane]" });
    expect(checkbox).toBeEnabled();
    expect(checkbox).not.toBeChecked();
    await user.click(checkbox);
    await user.click(await screen.findByRole("button", { name: "加入渲染队列" }));

    expect(enqueue).toHaveBeenCalledWith({
      client: "stable",
      replay_paths: ["C:\\osu!\\Replays\\new.osr"],
      preferences,
    });
    expect(start).not.toHaveBeenCalled();
    expect(onClose).toHaveBeenCalledOnce();
    expect(onNavigate).toHaveBeenCalledWith("/local/media/render");
  });
});
