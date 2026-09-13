import { describe, expect, it } from "vitest";
import { matchesMedalSearch } from "./medalTranslations";

describe("matchesMedalSearch", () => {
  it("matches both the original English name and its Chinese translation", () => {
    const medal = { Medal_ID: 1, Name: "500 Combo", Description: "Reach 500 combo" };

    expect(matchesMedalSearch(medal, "500 combo")).toBe(true);
    expect(matchesMedalSearch(medal, "500 连击")).toBe(true);
  });

  it("keeps matching source text when a medal has no translation", () => {
    const medal = { Medal_ID: 999999, Name: "Unique English Medal" };

    expect(matchesMedalSearch(medal, "english medal")).toBe(true);
    expect(matchesMedalSearch(medal, "does not exist")).toBe(false);
  });
});
