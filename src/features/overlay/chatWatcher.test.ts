import { describe, expect, it } from "vitest";
import type { OverlayChatEntry } from "../../shared/types/osu";
import { appendChatEntry, applyChatUpdate } from "./chatWatcher";

function entry(id: number, over: Partial<OverlayChatEntry> = {}): OverlayChatEntry {
  return {
    id,
    channel: "#mp_1",
    nick: `player${id}`,
    text: `message ${id}`,
    kind: "chat",
    ts_ms: id * 1000,
    ...over,
  };
}

describe("appendChatEntry", () => {
  it("appends in order and trims the buffer to the limit", () => {
    let entries: OverlayChatEntry[] = [];
    for (let id = 1; id <= 205; id++) entries = appendChatEntry(entries, entry(id));
    expect(entries).toHaveLength(200);
    expect(entries[0]!.id).toBe(6);
    expect(entries[entries.length - 1]!.id).toBe(205);
  });
});

describe("applyChatUpdate", () => {
  it("fills the translation of the matching id and keeps others intact", () => {
    const entries = [entry(1), entry(2), entry(3)];
    const updated = applyChatUpdate(entries, { id: 2, channel: "#mp_1", translation: "译文" });
    expect(updated[1]!.translation).toBe("译文");
    expect(updated[0]!.translation).toBeUndefined();
    // 原数组不被修改。
    expect(entries[1]!.translation).toBeUndefined();
  });

  it("ignores updates for evicted ids", () => {
    const entries = [entry(1)];
    const updated = applyChatUpdate(entries, { id: 99, channel: "#mp_1", translation: "gone" });
    expect(updated).toEqual(entries);
  });
});
