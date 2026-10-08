import { describe, expect, it } from "vitest";
import { draftInput, lobbyDraft, lobbyState, queryFromParams } from "./model";
import type { CommunityLobby } from "../../shared/types/osu";

describe("community behavior", () => {
  it("converts local form times to UTC and permits already-started activities", () => {
    const draft = { ...lobbyDraft(), title: " MP ", description: "一起玩", starts_at: "2026-10-08T18:00", ends_at: "2026-10-08T20:00" };
    const input = draftInput(draft, new Date("2026-10-08T19:00").getTime());
    expect(input.title).toBe("MP"); expect(input.starts_at).toBe(new Date(draft.starts_at).toISOString());
    expect(() => draftInput({ ...draft, ends_at: draft.starts_at }, new Date("2026-10-08T19:00").getTime())).toThrow();
  });
  it("keeps URL filters bounded to supported values", () => {
    expect(queryFromParams(new URLSearchParams("ruleset=bad&platform=unknown&activity_type=ranked&q=RomAI"))).toEqual({ activity_type: "ranked", q: "RomAI" });
  });
  it("expires at the server clock cutoff and preserves manual closure", () => {
    const lobby = { starts_at: "2026-10-08T11:00:00Z", ends_at: "2026-10-08T12:00:00Z", closed_at: null } as CommunityLobby;
    expect(lobbyState(lobby, new Date("2026-10-08T12:00:00Z"))).toBe("ended");
    expect(lobbyState({ ...lobby, closed_at: "2026-10-08T10:00:00Z" }, new Date("2026-10-08T11:00:00Z"))).toBe("closed");
  });
});
