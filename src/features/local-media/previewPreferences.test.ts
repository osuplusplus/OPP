import { beforeEach, describe, expect, it } from "vitest";
import { defaultLiveRenderOptions } from "./model";
import { loadLiveRenderOptions, normalizeLiveRenderOptions, saveLiveRenderOptions } from "./previewPreferences";

describe("live preview preferences", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  it("returns defaults when nothing is stored", () => {
    expect(loadLiveRenderOptions("stable")).toEqual(defaultLiveRenderOptions);
  });

  it("normalizes dirty values", () => {
    const options = normalizeLiveRenderOptions({
      hud: false,
      bgOpacity: 9,
      cursorSize: -1,
      audioOffset: "late",
      hd: "sometimes",
      resultsStyle: "osu",
      skinPath: 42,
      avatarPath: "/tmp/avatar.png",
      offsetHeatmap: true,
    });
    expect(options.hud).toBe(false);
    expect(options.bgOpacity).toBe(1);
    expect(options.cursorSize).toBe(0.1);
    expect(options.audioOffset).toBe(defaultLiveRenderOptions.audioOffset);
    expect(options.hd).toBe("auto");
    expect(options.resultsStyle).toBe("lazer");
    expect(options.skinPath).toBeNull();
    expect(options.avatarPath).toBeNull();
    expect(options.offsetHeatmap).toBe(true);
  });

  it("remembers settings per client without persisting the avatar path", () => {
    saveLiveRenderOptions("stable", {
      ...defaultLiveRenderOptions,
      hud: false,
      bgOpacity: 0.55,
      skinPath: "/skins/my-skin",
      avatarPath: "/tmp/avatar.png",
      showCursorRipples: true,
    });
    const stable = loadLiveRenderOptions("stable");
    expect(stable.hud).toBe(false);
    expect(stable.bgOpacity).toBe(0.55);
    expect(stable.skinPath).toBe("/skins/my-skin");
    expect(stable.showCursorRipples).toBe(true);
    expect(stable.avatarPath).toBeNull();
    // lazer 客户端不受 stable 设置影响。
    expect(loadLiveRenderOptions("lazer")).toEqual(defaultLiveRenderOptions);
  });
});
