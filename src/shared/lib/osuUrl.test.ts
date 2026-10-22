import { describe, expect, it } from "vitest";
import {
  absoluteOsuUrl,
  osuBeatmapDifficultyUrl,
  osuBeatmapsetUrl,
  osuBeatmapUrl,
  osuScoreUrl,
  osuUserUrl,
} from "./osuUrl";

describe("osuUrl", () => {
  it("builds canonical website links", () => {
    expect(osuUserUrl(11511458)).toBe("https://osu.ppy.sh/users/11511458");
    expect(osuBeatmapUrl(756290)).toBe("https://osu.ppy.sh/beatmaps/756290");
    expect(osuBeatmapsetUrl(348682)).toBe("https://osu.ppy.sh/beatmapsets/348682");
    expect(osuScoreUrl(1234567890)).toBe("https://osu.ppy.sh/scores/1234567890");
  });

  it("builds difficulty deep links with the ruleset anchor", () => {
    expect(osuBeatmapDifficultyUrl(348682, "osu", 756290)).toBe(
      "https://osu.ppy.sh/beatmapsets/348682#osu/756290",
    );
  });

  it("joins site-relative paths onto the base url", () => {
    expect(absoluteOsuUrl("/beatmapsets/1")).toBe("https://osu.ppy.sh/beatmapsets/1");
  });
});
