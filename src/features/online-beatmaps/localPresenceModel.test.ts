import { describe, expect, it } from "vitest";
import type { LocalBeatmapPresence, OnlineBeatmapset } from "../../shared/types/osu";
import { summarizeLocalPresence } from "./localPresenceModel";

const set = { beatmaps: [{ id: 10 }, { id: 11 }] } as OnlineBeatmapset;
const map = (statuses: LocalBeatmapPresence["status"][]) => new Map(statuses.map((status, index) => [10 + index, { beatmap_id: 10 + index, status, clients: status === "present" ? ["stable" as const] : [] }]));
describe("local ownership labels", () => {
  it("requires every difficulty before marking the complete set present", () => {
    expect(summarizeLocalPresence(set, map(["present", "present"])).label).toBe("本地已有");
    expect(summarizeLocalPresence(set, map(["present", "missing"])).label).toBe("部分已有 1/2");
    expect(summarizeLocalPresence(set, map(["present", "unknown"])).state).toBe("partial");
  });
  it("keeps unscanned, loading and incomplete responses unknown", () => {
    expect(summarizeLocalPresence(set, new Map()).state).toBe("unknown");
    expect(summarizeLocalPresence(set, map(["missing", "unknown"])).state).toBe("unknown");
    expect(summarizeLocalPresence(set, map(["missing", "missing"])).state).toBe("missing");
    expect(summarizeLocalPresence({ beatmaps: [] } as unknown as OnlineBeatmapset, new Map()).state).toBe("unknown");
  });
});
