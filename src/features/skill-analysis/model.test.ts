import { describe, expect, it } from "vitest";
import { orderedSkills, skillRadarData } from "./model";

describe("skill analysis model", () => {
  it("orders strongest and weakest dimensions deterministically", () => {
    const ordered = orderedSkills({ stamina: 10, tenacity: 80, agility: 20, accuracy: 30, precision: 40, reaction: 50, memory: 60, reading: 70 });
    expect(ordered[0].key).toBe("tenacity");
    expect(ordered[ordered.length - 1]?.key).toBe("stamina");
  });

  it("keeps all eight dimensions for the radar", () => {
    expect(skillRadarData({ stamina: 1, tenacity: 2, agility: 3, accuracy: 4, precision: 5, reaction: 6, memory: 7, reading: 8 })).toHaveLength(8);
  });
});
