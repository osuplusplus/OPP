import { describe, expect, it } from "vitest";
import type { SkillContribution } from "../../shared/types/osu";
import { coveragePercent, filterContributions, newlyComputed, orderedSkills, skillRadarData } from "./model";

const contribution = (overrides: Partial<SkillContribution>): SkillContribution => ({
  beatmap_id: 1,
  title: "Song",
  artist: "Artist",
  version: "Normal",
  creator: "Mapper",
  mods: [],
  pp: 100,
  accuracy: 0.98,
  combo: 500,
  max_combo: 600,
  misses: 0,
  weight: 0.5,
  source: "online",
  resource_id: null,
  skills: { stamina: 0, tenacity: 0, agility: 0, accuracy: 0, precision: 0, reaction: 0, memory: 0, reading: 0 },
  weighted_skills: { stamina: 0, tenacity: 0, agility: 0, accuracy: 0, precision: 0, reaction: 0, memory: 0, reading: 0 },
  error: null,
  ...overrides,
});

describe("skill analysis model", () => {
  it("orders strongest and weakest dimensions deterministically", () => {
    const ordered = orderedSkills({ stamina: 10, tenacity: 80, agility: 20, accuracy: 30, precision: 40, reaction: 50, memory: 60, reading: 70 });
    expect(ordered[0].key).toBe("tenacity");
    expect(ordered[ordered.length - 1]?.key).toBe("stamina");
  });

  it("keeps all eight dimensions for the radar", () => {
    expect(skillRadarData({ stamina: 1, tenacity: 2, agility: 3, accuracy: 4, precision: 5, reaction: 6, memory: 7, reading: 8 })).toHaveLength(8);
  });

  it("sorts by pp when no dimension focus is active", () => {
    const items = [contribution({ beatmap_id: 1, pp: 120 }), contribution({ beatmap_id: 2, pp: 300 }), contribution({ beatmap_id: 3, pp: null })];
    expect(filterContributions(items, { focus: "all", search: "" }).map((item) => item.beatmap_id)).toEqual([2, 1, 3]);
  });

  it("keeps only scores contributing to the focused dimension, sorted by contribution", () => {
    const items = [
      contribution({ beatmap_id: 1, pp: 500, weighted_skills: { stamina: 0, tenacity: 0, agility: 0, accuracy: 0, precision: 0, reaction: 0, memory: 0, reading: 10 } }),
      contribution({ beatmap_id: 2, pp: 100, weighted_skills: { stamina: 0, tenacity: 0, agility: 0, accuracy: 0, precision: 0, reaction: 0, memory: 0, reading: 40 } }),
      contribution({ beatmap_id: 3, pp: 200, weighted_skills: { stamina: 0, tenacity: 0, agility: 0, accuracy: 0, precision: 0, reaction: 0, memory: 0, reading: 0 } }),
    ];
    expect(filterContributions(items, { focus: "reading", search: "" }).map((item) => item.beatmap_id)).toEqual([2, 1]);
  });

  it("matches search against title, artist and version (case-insensitive)", () => {
    const items = [
      contribution({ beatmap_id: 1, title: "Freedom Dive", version: "Another" }),
      contribution({ beatmap_id: 2, artist: "xi", version: "Extra" }),
    ];
    expect(filterContributions(items, { focus: "all", search: "freedom" }).map((item) => item.beatmap_id)).toEqual([1]);
    expect(filterContributions(items, { focus: "all", search: " XI " }).map((item) => item.beatmap_id)).toEqual([2]);
    expect(filterContributions(items, { focus: "all", search: "extra" }).map((item) => item.beatmap_id)).toEqual([2]);
  });

  it("computes coverage percent and newly computed count defensively", () => {
    expect(coveragePercent({ requested_scores: 200, analyzed_scores: 192, reused_scores: 60, local_scores: 100, online_scores: 92, skipped_scores: 8, skipped_reasons: [] })).toBe(96);
    expect(coveragePercent({ requested_scores: 0, analyzed_scores: 0, reused_scores: 0, local_scores: 0, online_scores: 0, skipped_scores: 0, skipped_reasons: [] })).toBe(0);
    expect(newlyComputed({ requested_scores: 200, analyzed_scores: 192, reused_scores: 60, local_scores: 100, online_scores: 92, skipped_scores: 8, skipped_reasons: [] })).toBe(132);
  });
});
