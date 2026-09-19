import { describe, expect, it } from "vitest";
import { createStageQuery, extractStageHue, initialDifficulty, sortedDifficulties } from "./stageModel";
import { stageSet } from "./stageFixtures.test-data";

describe("local stage model", () => {
  it("preserves current mode and all backend filter fields", () => {
    expect(createStageQuery("lazer", "mania")).toMatchObject({ client: "lazer", rulesets: ["mania"], limit: 20, min_stars: null, max_length_ms: null });
  });
  it("selects a matched difficulty ahead of an easier non-match, with unknown stars last", () => {
    const set = { ...stageSet, difficulties: [...stageSet.difficulties, { ...stageSet.difficulties[0], stars: null, difficulty_name: "Unknown" }] };
    expect(sortedDifficulties(set).map((d) => d.difficulty_name)).toEqual(["Easy", "Insane", "Unknown"]);
    expect(initialDifficulty(set, new Set(["502"]))?.resource.resource_id).toBe("502");
    expect(initialDifficulty(set, new Set())?.resource.resource_id).toBe("501");
  });
  it("extracts chromatic accents instead of white, black, transparent or grayscale pixels", () => {
    expect(extractStageHue([255, 255, 255, 255, 0, 0, 0, 255, 100, 100, 100, 255])).toBeNull();
    expect(extractStageHue([255, 0, 0, 0])).toBeNull();
    expect(extractStageHue([255, 255, 255, 255, 30, 60, 220, 255])).toBe(225);
  });
});
