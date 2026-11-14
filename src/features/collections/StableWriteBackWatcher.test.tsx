import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { desktopApi } from "../../shared/lib/tauri";
import type { CollectionSyncStatus, GameStatusSnapshot } from "../../shared/types/osu";
import { StableWriteBackWatcher } from "./StableWriteBackWatcher";

vi.mock("../../shared/lib/tauri", () => ({
  desktopApi: {
    beginCollectionTask: vi.fn().mockResolvedValue(undefined),
    getGameStatus: vi.fn(),
    onGameStatusChanged: vi.fn(),
    getCollectionSyncStatus: vi.fn(),
    refreshCollections: vi.fn().mockResolvedValue({ folders: [] }),
    writeStableCollections: vi.fn().mockResolvedValue({ written_folders: 2, skipped_entries: 0, backup_path: null }),
  },
}));

const statusHandlers: Array<(status: GameStatusSnapshot) => void> = [];

const clients = (running: boolean): GameStatusSnapshot["clients"] => [
  { client: "stable", running, executable: "D:/osu!/osu!.exe", detected_at: "2026-11-14T00:00:00Z" },
];
const syncStatus = (over: Partial<CollectionSyncStatus>): CollectionSyncStatus => ({
  available: true, in_sync: false, pending_changes: true, game_changed: false,
  missing_downloadable_count: 0, missing_unresolved_count: 0, ...over,
});

function mount() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<QueryClientProvider client={client}><StableWriteBackWatcher /></QueryClientProvider>);
  return client;
}

/** 首次快照 stable 正在运行，随后广播退出：等待延迟后收尾。 */
async function runExit() {
  await act(async () => {
    statusHandlers.forEach((handler) => handler({ clients: clients(false) }));
  });
  await act(async () => { vi.advanceTimersByTimeAsync(4_000); });
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.clearAllMocks();
  statusHandlers.length = 0;
  vi.mocked(desktopApi.onGameStatusChanged).mockImplementation((handler) => {
    statusHandlers.push(handler);
    return Promise.resolve(() => undefined);
  });
  vi.mocked(desktopApi.getGameStatus).mockResolvedValue({ clients: clients(true) });
  vi.mocked(desktopApi.getCollectionSyncStatus).mockResolvedValue(syncStatus({}));
});

afterEach(() => { vi.useRealTimers(); });

it("writes pending collections back after osu!stable exits without touching game state", async () => {
  mount();
  await act(async () => { vi.runOnlyPendingTimersAsync(); });
  await runExit();
  expect(desktopApi.refreshCollections).not.toHaveBeenCalled();
  expect(desktopApi.writeStableCollections).toHaveBeenCalled();
});

it("merges game-side changes first when the game rewrote collection.db", async () => {
  // osu! 退出时用自己的内存回写 db（覆盖了 OPP 的写入）：必须先读取合并再写回。
  vi.mocked(desktopApi.getCollectionSyncStatus).mockResolvedValue(syncStatus({ pending_changes: false, game_changed: true }));
  mount();
  await act(async () => { vi.runOnlyPendingTimersAsync(); });
  await runExit();
  expect(desktopApi.refreshCollections).toHaveBeenCalledWith("stable");
  expect(desktopApi.writeStableCollections).toHaveBeenCalled();
});

it("heals a missed reconciliation on startup when stable is not running", async () => {
  vi.mocked(desktopApi.getGameStatus).mockResolvedValue({ clients: clients(false) });
  mount();
  await act(async () => { vi.advanceTimersByTimeAsync(4_000); });
  expect(desktopApi.writeStableCollections).toHaveBeenCalled();
});

it("skips reconciliation when everything is already in sync", async () => {
  vi.mocked(desktopApi.getCollectionSyncStatus).mockResolvedValue(syncStatus({ in_sync: true, pending_changes: false }));
  mount();
  await act(async () => { vi.runOnlyPendingTimersAsync(); });
  await runExit();
  expect(desktopApi.writeStableCollections).not.toHaveBeenCalled();
  // 游戏保持运行（无退出沿）时不应触发任何检查。
  await act(async () => {
    statusHandlers.forEach((handler) => handler({ clients: clients(true) }));
  });
  await act(async () => { vi.advanceTimersByTimeAsync(10_000); });
  expect(desktopApi.getCollectionSyncStatus).toHaveBeenCalledTimes(1);
});

it("surfaces write-back failures instead of failing silently", async () => {
  vi.mocked(desktopApi.writeStableCollections).mockRejectedValue({ code: "COLLECTION_CONFLICT", message: "指纹不一致" });
  mount();
  await act(async () => { vi.runOnlyPendingTimersAsync(); });
  await runExit();
  expect(desktopApi.writeStableCollections).toHaveBeenCalled();
});
