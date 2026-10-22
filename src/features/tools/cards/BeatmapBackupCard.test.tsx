import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { desktopApi } from "../../../shared/lib/tauri";
import type { BeatmapHubBackup } from "../../../shared/types/osu";
import { BeatmapBackupCard } from "./BeatmapBackupCard";

const backup: BeatmapHubBackup = {
  owner_user_id: "user-1",
  beatmapset_ids: [1, 2, 3, 4],
  count: 4,
  manifest_hash: "abc",
  created_at: "2026-10-01T08:00:00Z",
  updated_at: "2026-10-20T08:00:00Z",
  locally_available_count: 3,
  missing_count: 1,
};

function renderCard() {
  return render(<MemoryRouter><BeatmapBackupCard /></MemoryRouter>);
}

describe("BeatmapBackupCard", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("loads the cloud backup and renders local diff counts", async () => {
    vi.spyOn(desktopApi, "getBeatmapBackup").mockResolvedValue(backup);
    renderCard();

    expect(await screen.findByText("云端谱面集")).toBeInTheDocument();
    expect(screen.getByText("4")).toBeInTheDocument();
    expect(screen.getByText("本地缺失")).toBeInTheDocument();
    expect(screen.getByText("1")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /重新上传覆盖备份/ })).toBeEnabled();
    expect(screen.getByRole("button", { name: /删除云端备份/ })).toBeEnabled();
  });

  it("shows the empty hint instead of an error when no backup exists", async () => {
    vi.spyOn(desktopApi, "getBeatmapBackup").mockRejectedValue({
      code: "BACKUP_NOT_FOUND",
      message: "No beatmap backup exists for this account",
    });
    renderCard();

    expect(await screen.findByText(/云端还没有备份/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /上传本地谱面列表/ })).toBeEnabled();
    expect(screen.queryByRole("button", { name: /删除云端备份/ })).not.toBeInTheDocument();
  });

  it("uploads the local library and refreshes the summary", async () => {
    const fetch = vi.spyOn(desktopApi, "getBeatmapBackup")
      .mockRejectedValueOnce({ code: "BACKUP_NOT_FOUND", message: "none" })
      .mockResolvedValue(backup);
    const upload = vi.spyOn(desktopApi, "uploadBeatmapBackup").mockResolvedValue({
      count: 4,
      skipped: 0,
      manifest_hash: "abc",
      created_at: backup.created_at,
      updated_at: backup.updated_at,
    });
    renderCard();

    await userEvent.click(await screen.findByRole("button", { name: /上传本地谱面列表/ }));

    await waitFor(() => expect(upload).toHaveBeenCalledWith());
    expect(await screen.findByText(/已备份 4 个谱面集/)).toBeInTheDocument();
    expect(fetch).toHaveBeenCalledTimes(2);
    expect(screen.getByText("最近备份时间")).toBeInTheDocument();
  });

  it("requires confirmation before deleting the backup", async () => {
    vi.spyOn(desktopApi, "getBeatmapBackup").mockResolvedValue(backup);
    const remove = vi.spyOn(desktopApi, "deleteBeatmapBackup").mockResolvedValue(undefined);
    renderCard();

    await userEvent.click(await screen.findByRole("button", { name: /删除云端备份/ }));
    expect(remove).not.toHaveBeenCalled();

    await userEvent.click(await screen.findByRole("button", { name: /确认删除/ }));

    await waitFor(() => expect(remove).toHaveBeenCalledWith());
    expect(await screen.findByText(/云端备份已删除/)).toBeInTheDocument();
    expect(await screen.findByText(/云端还没有备份/)).toBeInTheDocument();
  });

  it("restores the backup into a collection and links to the collections page", async () => {
    vi.spyOn(desktopApi, "getBeatmapBackup").mockResolvedValue(backup);
    const restore = vi.spyOn(desktopApi, "restoreBeatmapBackup").mockResolvedValue({
      folder_id: "folder-1",
      folder_name: "谱面备份恢复 2026-10-21 11:40",
      total_sets: 4,
      local_sets: 3,
      missing_sets: 1,
      entries: 12,
    });
    renderCard();

    await userEvent.click(await screen.findByRole("button", { name: /恢复为收藏夹/ }));

    await waitFor(() => expect(restore).toHaveBeenCalledWith());
    expect(await screen.findByText(/本地已有的谱面集按难度完整导入/)).toBeInTheDocument();
    expect(screen.getByText(/共 12 条收藏条目/)).toBeInTheDocument();
    const link = screen.getByRole("link", { name: /去收藏夹查看并补齐缺失谱面/ });
    expect(link).toHaveAttribute("href", "/collections");
  });

  it("surfaces unexpected load failures with a retry panel", async () => {
    vi.spyOn(desktopApi, "getBeatmapBackup").mockRejectedValue({
      code: "HUB_HTTP_ERROR",
      message: "请求失败",
    });
    renderCard();

    expect(await screen.findByText(/请求失败（HUB_HTTP_ERROR）/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /重新加载/ })).toBeInTheDocument();
  });
});
