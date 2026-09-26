import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { desktopApi } from "../../shared/lib/tauri";
import type { BeatmapDownloadResult } from "../../shared/types/osu";
import { DownloadResultActions, DownloadTargetActions } from "./DownloadResultActions";

vi.mock("../../shared/lib/tauri", () => ({ desktopApi: { openDownloadedPath: vi.fn(), openExternal: vi.fn(), openBeatmapFiles: vi.fn() } }));
const result: BeatmapDownloadResult = { destination: "C:/Maps", total: 3, completed: 2, skipped: 0, failed: 1, cancelled: false, completed_paths: ["C:/Maps/1.osz", "C:/Maps/2.osz", "C:/Maps/1.osz"], failures: [{ beatmapset_id: 2007718, title: "Aurora", message: "network" }] };
beforeEach(() => { vi.resetAllMocks(); vi.mocked(desktopApi.openDownloadedPath).mockResolvedValue(undefined); vi.mocked(desktopApi.openExternal).mockResolvedValue(undefined); vi.mocked(desktopApi.openBeatmapFiles).mockResolvedValue({ opened: 2, failed: 0, failures: [] }); });

it("opens each saved archive only after the explicit action, and retries only failed paths", async () => {
  vi.mocked(desktopApi.openDownloadedPath).mockRejectedValueOnce(new Error("没有关联应用"));
  render(<DownloadResultActions result={result} />);
  expect(desktopApi.openDownloadedPath).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole("button", { name: "一键打开已下载曲包（2）" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("1.osz：没有关联应用");
  expect(vi.mocked(desktopApi.openDownloadedPath).mock.calls).toEqual([["C:/Maps/1.osz"], ["C:/Maps/2.osz"]]);
  await userEvent.click(screen.getByRole("button", { name: "重试打开失败曲包（1）" }));
  await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
  expect(desktopApi.openDownloadedPath).toHaveBeenLastCalledWith("C:/Maps/1.osz");
  expect(desktopApi.openDownloadedPath).toHaveBeenCalledTimes(3);
});

it("hands official recovery to the browser and displays launch errors", async () => {
  vi.mocked(desktopApi.openExternal).mockRejectedValueOnce(new Error("浏览器启动失败"));
  render(<DownloadResultActions result={result} />);
  await userEvent.click(screen.getByRole("button", { name: "去官网补下载：Aurora" }));
  expect(desktopApi.openExternal).toHaveBeenCalledWith("https://osu.ppy.sh/beatmapsets/2007718");
  expect(await screen.findByRole("alert")).toHaveTextContent("浏览器启动失败");
  expect(desktopApi.openDownloadedPath).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole("button", { name: "打开下载目录" }));
  expect(desktopApi.openDownloadedPath).toHaveBeenCalledWith("C:/Maps");
});

it("keeps cancelled successful downloads openable and prevents duplicate in-flight handoff", async () => {
  let finish!: () => void;
  vi.mocked(desktopApi.openDownloadedPath).mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
  render(<DownloadResultActions result={{ ...result, cancelled: true, completed_paths: ["C:/Maps/1.osz"] }} />);
  await userEvent.click(screen.getByRole("button", { name: "一键打开已下载曲包（1）" }));
  expect(screen.getByRole("button", { name: "正在打开曲包…" })).toBeDisabled();
  finish();
  expect(await screen.findByRole("status")).toHaveTextContent("已交给系统打开 1 个曲包");
});

it("opens completed archives through the selected osu! client", async () => {
  const onNotice = vi.fn();
  render(<DownloadTargetActions result={result} onNotice={onNotice} />);
  await userEvent.click(screen.getByRole("button", { name: "一键导入 lazer（2）" }));
  await waitFor(() => expect(desktopApi.openBeatmapFiles).toHaveBeenCalledWith("lazer", ["C:/Maps/1.osz", "C:/Maps/2.osz"]));
  expect(onNotice).toHaveBeenCalledWith("已交给 lazer 导入 2 个曲包。");
  await userEvent.click(screen.getByRole("button", { name: "一键导入 Stable（2）" }));
  await waitFor(() => expect(desktopApi.openBeatmapFiles).toHaveBeenCalledWith("stable", ["C:/Maps/1.osz", "C:/Maps/2.osz"]));
});

it("keeps the download directory available beside the client actions", async () => {
  const onNotice = vi.fn();
  render(<DownloadTargetActions result={result} onNotice={onNotice} />);
  await userEvent.click(screen.getByRole("button", { name: "打开下载目录" }));
  await waitFor(() => expect(desktopApi.openDownloadedPath).toHaveBeenCalledWith("C:/Maps"));
  expect(onNotice).toHaveBeenCalledWith("已打开下载目录。");
});
