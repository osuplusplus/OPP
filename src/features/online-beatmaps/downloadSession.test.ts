import { describe, expect, it, vi } from "vitest";
import type { BeatmapDownloadProgress, BeatmapDownloadResult, OnlineBeatmapset } from "../../shared/types/osu";
import { createDownloadSession } from "./downloadSession";

const set = (id: number, blocked = false): OnlineBeatmapset => ({ id, artist: "Artist", title: `Song ${id}`, creator: "Mapper", status: "ranked", availability: { download_disabled: blocked } });
const options = async () => ({ destination: "C:/Maps", provider: "sayobot" as const, overwrite: false, include_video: true });
function setup() {
  let finish!: (result: BeatmapDownloadResult) => void;
  let emit!: (progress: BeatmapDownloadProgress) => void;
  const dispose = vi.fn();
  const api = {
    downloadOnlineBeatmapsets: vi.fn(() => new Promise<BeatmapDownloadResult>((resolve) => { finish = resolve; })),
    onBeatmapDownloadProgress: vi.fn(async (listener: (progress: BeatmapDownloadProgress) => void) => { emit = listener; return dispose; }),
    cancelOnlineBeatmapDownload: vi.fn(async () => undefined),
  };
  return { session: createDownloadSession(api), api, dispose, finish: (result: Partial<BeatmapDownloadResult>) => finish({ destination: "C:/Maps", total: 3, completed: 0, skipped: 0, failed: 0, cancelled: false, failures: [], ...result }), emit: (progress: Partial<BeatmapDownloadProgress>) => emit({ phase: "downloading", total: 3, processed: 0, completed: 0, skipped: 0, failed: 0, current_beatmapset_id: null, current_title: null, message: null, ...progress }) };
}
describe("online download session", () => {
  it("passes subset validation explicitly for tournament selections", async () => {
    const { session, api, finish } = setup();
    const run = session.start([{ ...set(20), beatmaps: [{ id: 10 }, { id: 11 }], allow_extra_difficulties: true }], options);
    await vi.waitFor(() => expect(api.downloadOnlineBeatmapsets).toHaveBeenCalledOnce());
    expect(api.downloadOnlineBeatmapsets).toHaveBeenCalledWith(expect.objectContaining({ items: [expect.objectContaining({
      beatmapset_id: 20, expected_beatmap_ids: [10, 11], allow_extra_difficulties: true,
    })] }));
    finish({ total: 1, completed: 1 }); await run;
  });
  it("passes the current difficulty IDs to archive validation", async () => {
    const { session, api, finish } = setup();
    const item = { ...set(2419109), beatmaps: [{ id: 5589234 }, { id: 5589235 }] } as OnlineBeatmapset;
    const run = session.start([item], options);
    await vi.waitFor(() => expect(api.downloadOnlineBeatmapsets).toHaveBeenCalledOnce());
    expect(api.downloadOnlineBeatmapsets).toHaveBeenCalledWith(expect.objectContaining({ items: [{ beatmapset_id: 2419109, artist: "Artist", title: "Song 2419109", expected_beatmap_ids: [5589234, 5589235] }] }));
    finish({ total: 1, completed: 1 });
    await run;
  });
  it("merges different searches, deduplicates and excludes prohibited sets", () => {
    const { session } = setup();
    session.add([set(1), set(2)]);
    expect(session.add([set(2), set(3), set(4, true)])).toEqual({ added: 1, duplicates: 1, blocked: 1 });
    expect(session.getSnapshot().queue.map((item) => item.id)).toEqual([1, 2, 3]);
    const unmount = session.subscribe(vi.fn()); unmount();
    expect(session.getSnapshot().queue).toHaveLength(3);
  });
  it("freezes the active batch, keeps new additions and failed or unprocessed items after cancellation", async () => {
    const { session, api, finish, dispose } = setup();
    session.add([set(1), set(2), set(3), set(4)]);
    const run = session.start(session.getSnapshot().queue, options);
    await vi.waitFor(() => expect(api.downloadOnlineBeatmapsets).toHaveBeenCalledOnce());
    session.add([set(5)]); session.remove(1);
    await session.start([set(6)], options);
    expect(api.downloadOnlineBeatmapsets).toHaveBeenCalledOnce();
    expect(api.downloadOnlineBeatmapsets.mock.calls[0]).toEqual([expect.objectContaining({ items: [1, 2, 3, 4].map((id) => ({ beatmapset_id: id, artist: "Artist", title: `Song ${id}` })) })]);
    await session.cancel();
    finish({ total: 4, completed: 1, skipped: 1, failed: 1, cancelled: true, failures: [{ beatmapset_id: 3, title: "Song 3", message: "network" }] });
    await run;
    expect(session.getSnapshot().queue.map((item) => item.id)).toEqual([3, 4, 5]);
    expect(api.cancelOnlineBeatmapDownload).toHaveBeenCalledOnce();
    expect(dispose).toHaveBeenCalledOnce();
  });
  it("keeps progress running without UI subscribers and releases the lock if preparation is cancelled", async () => {
    const { session, api, emit, finish } = setup();
    session.add([set(1)]);
    await session.start(session.getSnapshot().queue, async () => null);
    expect(session.getSnapshot().busy).toBe(false);
    expect(api.downloadOnlineBeatmapsets).not.toHaveBeenCalled();
    const unmount = session.subscribe(vi.fn());
    const run = session.start(session.getSnapshot().queue, options);
    await vi.waitFor(() => expect(api.downloadOnlineBeatmapsets).toHaveBeenCalledOnce()); unmount();
    emit({ total: 1, current_beatmapset_id: 1, processed: 1, completed: 1, phase: "completed" });
    expect(session.getSnapshot().progress?.completed).toBe(1);
    finish({ total: 1, completed: 1 }); await run;
    expect(session.getSnapshot().queue).toEqual([]);
  });
  it("preserves the queue on subscription or preparation failure", async () => {
    const { session, api } = setup(); session.add([set(1)]);
    api.onBeatmapDownloadProgress.mockRejectedValueOnce(new Error("listener unavailable"));
    await session.start(session.getSnapshot().queue, options);
    expect(session.getSnapshot()).toMatchObject({ busy: false, error: "listener unavailable" });
    expect(session.getSnapshot().queue).toHaveLength(1);
    expect(api.downloadOnlineBeatmapsets).not.toHaveBeenCalled();
  });
  it("does not start a download when cancelled while choosing its destination", async () => {
    const { session, api } = setup(); session.add([set(1)]);
    let choose!: (value: Awaited<ReturnType<typeof options>>) => void;
    const run = session.start(session.getSnapshot().queue, () => new Promise((resolve) => { choose = resolve; }));
    await session.cancel(); choose(await options()); await run;
    expect(api.downloadOnlineBeatmapsets).not.toHaveBeenCalled();
    expect(session.getSnapshot()).toMatchObject({ busy: false, queue: [set(1)] });
  });
});
