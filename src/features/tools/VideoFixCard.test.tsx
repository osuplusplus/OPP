import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { desktopApi } from "../../shared/lib/tauri";
import type { VideoFixResult, VideoScanResult } from "../../shared/types/osu";
import { VideoFixCard } from "./VideoFixCard";

const scanResult: VideoScanResult = {
  songs_root: "/osu/Songs",
  video_count: 3,
  video_size: 3 * 1024 * 1024,
  pending_count: 2,
  pending_size: 2 * 1024 * 1024,
  pending: [
    { path: "/osu/Songs/100 map/video.mp4", size: 1024 * 1024 },
    { path: "/osu/Songs/200 map/background.m4v", size: 1024 * 1024 },
  ],
  pending_truncated: false,
  already_flv_count: 1,
  empty_count: 0,
  other_count: 0,
};

const fixResult: VideoFixResult = {
  songs_root: "/osu/Songs",
  cancelled: false,
  fixed_count: 2,
  fixed_size: 2 * 1024 * 1024,
  remuxed_count: 1,
  transcoded_count: 1,
  skipped_count: 1,
  failed_count: 1,
  failed: [{ path: "/osu/Songs/300 map/video.mp4", message: "二次转码失败" }],
};

describe("VideoFixCard", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.spyOn(desktopApi, "liveRenderGetFfmpegStatus").mockResolvedValue({
      path: "/usr/bin/ffmpeg",
      version: "ffmpeg version 7.1",
      source: "path",
    });
  });

  it("scans the detected songs directory and requires acknowledgement before fixing", async () => {
    const scan = vi.spyOn(desktopApi, "scanOsuVideos").mockResolvedValue(scanResult);
    render(<VideoFixCard />);

    await userEvent.click(screen.getByRole("button", { name: /扫描视频/ }));

    await waitFor(() => expect(scan).toHaveBeenCalledWith(null));
    expect(await screen.findByText("待修复")).toBeInTheDocument();
    expect(screen.getByText("/osu/Songs/100 map/video.mp4 — 1.0M")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /修复 2 个视频/ })).toBeDisabled();

    await userEvent.click(screen.getByRole("checkbox"));
    expect(screen.getByRole("button", { name: /修复 2 个视频/ })).toBeEnabled();
  });

  it("confirms and runs the fix, then renders the per-file failure list", async () => {
    vi.spyOn(desktopApi, "scanOsuVideos").mockResolvedValue(scanResult);
    const fix = vi.spyOn(desktopApi, "fixOsuVideos").mockResolvedValue(fixResult);
    render(<VideoFixCard />);

    await userEvent.click(screen.getByRole("button", { name: /扫描视频/ }));
    await userEvent.click(await screen.findByRole("checkbox"));
    await userEvent.click(screen.getByRole("button", { name: /修复 2 个视频/ }));
    await userEvent.click(await screen.findByRole("button", { name: /确认修复/ }));

    await waitFor(() => expect(fix).toHaveBeenCalledWith(null));
    expect(await screen.findByText("已修复")).toBeInTheDocument();
    expect(screen.getByText("有损二次转码")).toBeInTheDocument();
    expect(screen.getByText(/1 个视频处理失败/)).toBeInTheDocument();
    expect(screen.getByText(/300 map\/video.mp4 — 二次转码失败/)).toBeInTheDocument();
  });

  it("uses the manually selected directory for scanning", async () => {
    vi.spyOn(desktopApi, "chooseDirectory").mockResolvedValue("/mnt/osu/Songs");
    const scan = vi.spyOn(desktopApi, "scanOsuVideos").mockResolvedValue(scanResult);
    render(<VideoFixCard />);

    await userEvent.click(screen.getByRole("button", { name: /选择其他目录/ }));
    await userEvent.click(screen.getByRole("button", { name: /扫描视频/ }));

    await waitFor(() => expect(scan).toHaveBeenCalledWith("/mnt/osu/Songs"));
    expect(await screen.findByText("扫描目录：/osu/Songs")).toBeInTheDocument();
  });
});
