import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ComponentProps } from "react";

import { desktopApi, type UpdateCheckResult } from "../../shared/lib/tauri";
import { requestManualUpdateCheck } from "./events";
import { resetUpdateCheckSessionForTests, shouldShowAutomaticUpdate } from "./check";
import { UpdateCenter } from "./UpdateCenter";

vi.mock("../../shared/lib/tauri", () => ({
  desktopApi: {
    checkForUpdates: vi.fn(),
    downloadAndInstallUpdate: vi.fn(),
    ignoreUpdateVersion: vi.fn(),
    onUpdateProgress: vi.fn().mockResolvedValue(() => undefined),
    openExternal: vi.fn(),
  },
}));

const update: UpdateCheckResult = {
  current_version: "0.4.1",
  latest_version: "0.5.0",
  latest_tag: "v0.5.0",
  is_latest: false,
  release_name: "OPP v0.5.0",
  release_url: "https://github.com/osuplusplus/OPP/releases/tag/v0.5.0",
  published_at: "2026-08-13T00:00:00Z",
  release_notes: "- 启动自动检查更新\n- 新增更新公告",
  can_auto_update: true,
  download_size: 80 * 1024 * 1024,
};

function renderCenter(props?: Partial<ComponentProps<typeof UpdateCenter>>) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <UpdateCenter autoCheckDelayMs={0} autoCheckReady={false} {...props} />
    </QueryClientProvider>,
  );
}

describe("shouldShowAutomaticUpdate", () => {
  it("only shows a newer version that has not been ignored", () => {
    expect(shouldShowAutomaticUpdate(update, null)).toBe(true);
    expect(shouldShowAutomaticUpdate(update, "0.5.0")).toBe(false);
    expect(shouldShowAutomaticUpdate({ ...update, is_latest: true }, null)).toBe(false);
  });
});

describe("UpdateCenter", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetUpdateCheckSessionForTests();
  });

  it("deduplicates concurrent automatic checks during one startup", async () => {
    vi.mocked(desktopApi.checkForUpdates).mockReturnValue(
      new Promise<UpdateCheckResult>(() => undefined),
    );
    const first = renderCenter({ autoCheckReady: true });
    const second = renderCenter({ autoCheckReady: true });

    await waitFor(() => expect(desktopApi.checkForUpdates).toHaveBeenCalledOnce());
    first.unmount();
    second.unmount();
  });

  it("keeps automatic checks quiet when the version is current or ignored", async () => {
    vi.mocked(desktopApi.checkForUpdates).mockResolvedValue({ ...update, is_latest: true });
    const current = renderCenter({ autoCheckReady: true });
    await waitFor(() => expect(desktopApi.checkForUpdates).toHaveBeenCalledOnce());
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    current.unmount();

    resetUpdateCheckSessionForTests();
    vi.mocked(desktopApi.checkForUpdates).mockResolvedValue(update);
    renderCenter({ autoCheckReady: true, ignoredVersion: "0.5.0" });
    await waitFor(() => expect(desktopApi.checkForUpdates).toHaveBeenCalledTimes(2));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("always reports the result of a manual check", async () => {
    vi.mocked(desktopApi.checkForUpdates).mockResolvedValue({ ...update, is_latest: true });
    renderCenter();

    act(() => requestManualUpdateCheck());

    expect(await screen.findByRole("heading", { name: "已是最新版本" })).toBeInTheDocument();
  });

  it("supports later, ignore, and in-app update actions", async () => {
    const user = userEvent.setup();
    vi.mocked(desktopApi.checkForUpdates).mockResolvedValue(update);
    vi.mocked(desktopApi.ignoreUpdateVersion).mockResolvedValue({
      ignored_update_version: "0.5.0",
    } as never);
    renderCenter();

    act(() => requestManualUpdateCheck());
    await screen.findByText("本次更新内容");
    await user.click(screen.getAllByRole("button", { name: "下次再说" })[1]);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(desktopApi.ignoreUpdateVersion).not.toHaveBeenCalled();

    requestManualUpdateCheck();
    await screen.findByText("本次更新内容");
    await user.click(screen.getByRole("button", { name: "忽略此版本" }));
    await waitFor(() => expect(desktopApi.ignoreUpdateVersion).toHaveBeenCalledWith("0.5.0"));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    requestManualUpdateCheck();
    await screen.findByText("本次更新内容");
    await user.click(screen.getByRole("button", { name: "立即更新" }));
    expect(desktopApi.downloadAndInstallUpdate).toHaveBeenCalledWith("0.5.0");
    expect(desktopApi.openExternal).not.toHaveBeenCalled();
  });

  it("falls back to the release page when this platform cannot replace itself", async () => {
    const user = userEvent.setup();
    vi.mocked(desktopApi.checkForUpdates).mockResolvedValue({
      ...update,
      can_auto_update: false,
    });
    renderCenter();

    requestManualUpdateCheck();
    await screen.findByText("本次更新内容");
    await user.click(screen.getByRole("button", { name: "前往更新" }));

    expect(desktopApi.openExternal).toHaveBeenCalledWith(update.release_url);
  });

  it("shows download progress and keeps the dialog open while replacing", async () => {
    const user = userEvent.setup();
    let progressHandler: ((progress: {
      phase: "downloading";
      downloaded_bytes: number;
      total_bytes: number;
      message: string;
    }) => void) | undefined;
    vi.mocked(desktopApi.onUpdateProgress).mockImplementationOnce(async (handler) => {
      progressHandler = handler;
      return () => undefined;
    });
    vi.mocked(desktopApi.checkForUpdates).mockResolvedValue(update);
    vi.mocked(desktopApi.downloadAndInstallUpdate).mockReturnValue(
      new Promise<void>(() => undefined),
    );
    renderCenter();

    act(() => requestManualUpdateCheck());
    await screen.findByText("本次更新内容");
    await user.click(screen.getByRole("button", { name: "立即更新" }));
    await waitFor(() => expect(progressHandler).toBeDefined());
    act(() => progressHandler?.({
      phase: "downloading",
      downloaded_bytes: 40 * 1024 * 1024,
      total_bytes: 80 * 1024 * 1024,
      message: "正在下载新版本",
    }));

    expect(screen.getByText("50%")).toBeInTheDocument();
    expect(screen.getByText("40.0 MB / 80.0 MB")).toBeInTheDocument();
    for (const button of screen.getAllByRole("button", { name: "下次再说" })) {
      expect(button).toBeDisabled();
    }
  });

  it("reports a failed update download without closing the dialog", async () => {
    const user = userEvent.setup();
    vi.mocked(desktopApi.checkForUpdates).mockResolvedValue(update);
    vi.mocked(desktopApi.downloadAndInstallUpdate).mockRejectedValue({
      code: "NETWORK_ERROR",
      message: "更新包下载失败",
    });
    renderCenter();

    act(() => requestManualUpdateCheck());
    await screen.findByText("本次更新内容");
    await user.click(screen.getByRole("button", { name: "立即更新" }));

    expect(await screen.findByText("更新包下载失败")).toBeInTheDocument();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("treats Escape and the overlay as 'later' without ignoring", async () => {
    vi.mocked(desktopApi.checkForUpdates).mockResolvedValue(update);
    renderCenter();

    requestManualUpdateCheck();
    await screen.findByRole("dialog");
    fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());

    requestManualUpdateCheck();
    await screen.findByRole("dialog");
    fireEvent.pointerDown(screen.getByTestId("update-dialog-overlay"));
    fireEvent.click(screen.getByTestId("update-dialog-overlay"));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(desktopApi.ignoreUpdateVersion).not.toHaveBeenCalled();
  });

  it("shows manual failures and lets the user retry", async () => {
    const user = userEvent.setup();
    vi.mocked(desktopApi.checkForUpdates)
      .mockRejectedValueOnce({ code: "NETWORK_ERROR", message: "无法连接 GitHub" })
      .mockResolvedValueOnce(update);
    renderCenter();

    requestManualUpdateCheck();
    expect(await screen.findByText("无法连接 GitHub")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "重试" }));
    expect(await screen.findByText("本次更新内容")).toBeInTheDocument();
  });
});
