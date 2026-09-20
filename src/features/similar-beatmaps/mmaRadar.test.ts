import { describe, expect, it } from "vitest";

import type { ManiaPatternView } from "../../shared/types/osu";
import { mmaRadarValues } from "./mmaRadar";

const view = {
  category: "Shield",
  mode_tag: "Mix",
  coverage: [0.01, 0.04, 0.25, 0.5, 1, 0],
  bars: [
    { pattern: "Jacks", amount: 999_999, relative: 1, specific_types: [["Chordjacks", 0.2], ["Minijacks", 0.1]] },
    { pattern: "Stream", amount: 12_000, relative: 0.4, specific_types: [["Light Stream", 0.3]] },
  ],
  subtypes: [],
  ln_note_ratio: 0.234,
  intensity: [6.4, 12.1, 5.2, 18.5],
  temporal: [0.42, 0.18, 0.12],
  duration_seconds: 120,
  sv_amount: 0,
} satisfies ManiaPatternView;

describe("MMA coverage radar", () => {
  it("uses fixed square-root radii and real coverage seconds, ignoring merged bar amounts", () => {
    const values = mmaRadarValues(view);
    expect(values.map((value) => value.dimension)).toEqual(["Stream", "Chordstream", "Jacks", "Coordination", "Density", "Wildcard"]);
    expect(values.map((value) => value.radius)).toEqual([0.1, 0.2, 0.5, Math.sqrt(0.5), 1, 0]);
    expect(values[2]).toMatchObject({ coverage: 0.25, seconds: 30, specifics: "Chordjacks · Minijacks" });
  });

  it("keeps the same scale for a chart with smaller coverage", () => {
    const values = mmaRadarValues({ ...view, coverage: [0.01, 0.04, 0.25, 0, 0, 0] });
    expect(values[2].radius).toBe(0.5);
    expect(values[3].coverage).toBe(0);
    expect(values[3].seconds).toBe(0);
    expect(values[5].radius).toBe(0);
  });

  it("does not fabricate coverage from missing data", () => {
    expect(mmaRadarValues(undefined).every((value) => value.coverage === null && value.seconds === null)).toBe(true);
    expect(mmaRadarValues({ ...view, coverage: [] }).every((value) => value.radius === 0)).toBe(true);
  });

});
