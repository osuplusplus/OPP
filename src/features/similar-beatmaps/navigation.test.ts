import { describe, expect, it } from "vitest";

import {
  onlineBeatmapRouteForSimilarityResult,
  parseSimilarityLaunch,
  similarityRouteForBeatmap,
  similarityRouteForLocalResource,
} from "./navigation";

describe("similarity navigation", () => {
  it("opens a result with its exact difficulty selected", () => {
    expect(
      onlineBeatmapRouteForSimilarityResult({
        beatmapset_id: 2,
        beatmap_id: 20,
      }),
    ).toBe("/online/beatmaps?beatmapset=2&beatmap=20");
  });

  it("creates and parses an online beatmap launch", () => {
    expect(similarityRouteForBeatmap(123)).toBe("/online/similar?source=beatmap_id&value=123&ruleset=osu");
    expect(parseSimilarityLaunch(new URLSearchParams("source=beatmap_id&value=123")))
      .toEqual({ kind: "beatmap_id", beatmapId: "123", ruleset: "osu" });
    expect(parseSimilarityLaunch(new URLSearchParams("source=beatmap_id&value=456&ruleset=mania")))
      .toEqual({ kind: "beatmap_id", beatmapId: "456", ruleset: "mania" });
  });

  it("creates and parses a local resource launch", () => {
    const route = similarityRouteForLocalResource("stable", "stable:beatmap:abc 123");
    expect(route).toBe("/online/similar?source=local_resource&client=stable&resource=stable%3Abeatmap%3Aabc+123&ruleset=osu");
    expect(parseSimilarityLaunch(new URLSearchParams(route.split("?")[1])))
      .toEqual({ kind: "local_resource", client: "stable", resourceId: "stable:beatmap:abc 123", ruleset: "osu" });

    const maniaRoute = similarityRouteForLocalResource("lazer", "mania-map", "mania");
    expect(parseSimilarityLaunch(new URLSearchParams(maniaRoute.split("?")[1])))
      .toEqual({ kind: "local_resource", client: "lazer", resourceId: "mania-map", ruleset: "mania" });
  });
});
