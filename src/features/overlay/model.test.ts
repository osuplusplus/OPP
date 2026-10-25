import { describe, expect, it } from "vitest";
import {
  chatNickColor,
  compactCount,
  displayTitle,
  ircNotice,
  isMultiplayerState,
  resolveDownloadProvider,
  starColor,
  statusMeta,
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

describe("chatNickColor", () => {
  it("is stable per nick and drawn from the palette", () => {
    const first = chatNickColor("Player_Name");
    expect(chatNickColor("Player_Name")).toBe(first);
    expect(chatNickColor("other")).toMatch(/^#[0-9a-f]{6}$/);
    // 不同昵称大概率不同色；此处只验证哈希确实参与选择（至少存在差异）。
    const colors = new Set(
      ["a", "b", "c", "d", "e", "f", "g", "h"].map((nick) => chatNickColor(nick)),
    );
    expect(colors.size).toBeGreaterThan(1);
  });
});

describe("starColor", () => {
  it("maps star ratings to the website-like hue ramp", () => {
    // 低星偏蓝（hue 210），7★ 红（hue 0），8★ 以上转品红/紫。
    expect(starColor(0)).toBe("hsl(210 78% 62%)");
    expect(starColor(7)).toBe("hsl(0 78% 62%)");
    expect(starColor(8)).toBe("hsl(300 78% 62%)");
    // 超出范围钳制到同一端点。
    expect(starColor(12)).toBe(starColor(9));
    expect(starColor(-1)).toBe(starColor(0));
    // 中间值单调递减（蓝 → 红）。
    const hueOf = (value: string) => Number(value.match(/hsl\((\d+)/)![1]);
    expect(hueOf(starColor(2))).toBeGreaterThan(hueOf(starColor(3)));
  });
});

describe("statusMeta", () => {
  it("labels known statuses and falls back to unknown", () => {
    expect(statusMeta("ranked").label).toBe("上架");
    expect(statusMeta("loved").label).toBe("Loved");
    expect(statusMeta("graveyard").label).toBe("坟场");
    expect(statusMeta("whatever")).toEqual({ label: "未知", color: "#8f8f8f" });
    expect(statusMeta(null).label).toBe("未知");
  });
});

describe("compactCount", () => {
  it("abbreviates large counts", () => {
    expect(compactCount(null)).toBe("—");
    expect(compactCount(999)).toBe("999");
    expect(compactCount(12_345)).toBe("1.2万");
    expect(compactCount(234_000_000)).toBe("2.3亿");
  });
});

describe("ircNotice", () => {
  it("warns on missing credentials and errors, explains otherwise, silent when fine", () => {
    expect(ircNotice(null)).toBeNull();
    expect(ircNotice({ irc: { state: "connected", message: "" }, ircConfigured: true })).toBeNull();
    expect(
      ircNotice({ irc: { state: "error", message: "认证失败" }, ircConfigured: true })?.tone,
    ).toBe("warn");
    expect(
      ircNotice({ irc: { state: "disconnected", message: "" }, ircConfigured: false })?.text,
    ).toContain("IRC 未配置");
    expect(
      ircNotice({ irc: { state: "connecting", message: "" }, ircConfigured: true })?.tone,
    ).toBe("info");
    expect(
      ircNotice({ irc: { state: "disconnected", message: "" }, ircConfigured: true })?.tone,
    ).toBe("info");
  });
});
