import { expect, it, vi } from "vitest";
import { createPoolImportSession } from "./importSession";
import type { TournamentLink } from "../../shared/types/osu";
const link = (id: number): TournamentLink => ({ id, reference: { provider: "opp", url: `https://example.com/${id}.json` } });
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>((done) => { resolve = done; }); return { promise, resolve }; }

it("serializes imports, keeps only the latest queued link and ignores stale progress", async () => {
  const first = deferred<{ folder_id: string; existing: boolean }>();
  const api = { openTournamentPool: vi.fn().mockReturnValueOnce(first.promise).mockResolvedValue({ folder_id: "third", existing: false }) };
  const session = createPoolImportSession(api);
  expect(session.receive(link(1))).toBe(true);
  session.receive(link(2)); session.receive(link(3));
  expect(api.openTournamentPool).toHaveBeenCalledTimes(1);
  session.progress({ request_id: 1, phase: "saving" });
  expect(session.getSnapshot()?.link.id).toBe(3);
  expect(session.getSnapshot()?.phase).toBe("queued");
  first.resolve({ folder_id: "first", existing: false });
  await first.promise; await Promise.resolve();
  expect(api.openTournamentPool).toHaveBeenCalledTimes(2);
  expect(api.openTournamentPool).toHaveBeenLastCalledWith(link(3).reference, 3);
  expect(session.getSnapshot()).toMatchObject({ folderId: "third", phase: "completed" });
  expect(session.receive(link(2))).toBe(false);
});

it("retains failure for retry and supports opening an already saved snapshot", async () => {
  const api = { openTournamentPool: vi.fn().mockRejectedValueOnce({ message: "连接失败" }).mockResolvedValue({ folder_id: "offline", existing: true }) };
  const session = createPoolImportSession(api);
  session.receive(link(1)); await Promise.resolve();
  expect(session.getSnapshot()).toMatchObject({ phase: "failed", error: "连接失败" });
  session.retry(); await Promise.resolve();
  expect(session.getSnapshot()).toMatchObject({ phase: "completed", folderId: "offline", existing: true });
  session.progress({ request_id: 1, phase: "enriching" });
  expect(session.getSnapshot()?.phase).toBe("completed");
  session.dismiss(); expect(session.getSnapshot()).toBeNull();
});
