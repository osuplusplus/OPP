import { describe, expect, it } from "vitest";
import {
  displayTitle,
  isMultiplayerState,
  resolveDownloadProvider,
} from "./model";

describe("isMultiplayerState", () => {
  it("matches multiplayer and lobby states case-insensitively", () => {
    expect(isMultiplayerState("MultiplayerLobby")).toBe(true);
    expect(isMultiplayerState("multiplayergameplay")).toBe(true);
    expect(isMultiplayerState("Lobby")).toBe(true);
    expect(isMultiplayerState("SongSelect")).toBe(false);
    expect(isMultiplayerState(null)).toBe(false);
    expect(isMultiplayerState(undefined)).toBe(false);
  });
});

describe("resolveDownloadProvider", () => {
  it("keeps known providers and defaults to sayobot", () => {
    expect(resolveDownloadProvider({ default_beatmap_download_provider: "catboy" } as never)).toBe(
      "catboy",
    );
    expect(resolveDownloadProvider({ default_beatmap_download_provider: "hinai" } as never)).toBe(
      "hinai",
    );
    expect(resolveDownloadProvider(null)).toBe("sayobot");
    expect(resolveDownloadProvider(undefined)).toBe("sayobot");
  });
});

describe("displayTitle", () => {
  it("joins artist and title with fallbacks", () => {
    expect(displayTitle("A", "T")).toBe("A - T");
    expect(displayTitle(null, "T")).toBe("T");
    expect(displayTitle(" ", "T")).toBe("T");
    expect(displayTitle("A", null)).toBe("未知谱面");
  });
});
