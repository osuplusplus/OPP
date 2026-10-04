import { describe, expect, it } from "vitest";
import { byteSize, compactNumber, duration, errorMessage, scoreMods } from "./format";
import type { Score } from "../types/osu";

describe("format helpers", () => {
  it("does not turn missing values into zero", () => {
    expect(compactNumber(undefined)).toBe("—");
    expect(duration(null)).toBe("—");
    expect(byteSize(null)).toBe("—");
  });

  it("scales byte sizes in 1024 steps", () => {
    expect(byteSize(0)).toBe("0B");
    expect(byteSize(987)).toBe("987B");
    expect(byteSize(1_536)).toBe("1.5K");
    expect(byteSize(200 * 1024 * 1024)).toBe("200M");
    expect(byteSize(3 * 1024 * 1024 * 1024)).toBe("3.0G");
  });

  it("keeps play time in hours instead of converting it to days", () => {
    expect(duration(5_400)).toBe("1.5 小时");
    expect(duration(90_000)).toBe("25 小时");
    expect(duration(4_442_400)).toBe("1,234 小时");
  });

  it("handles modern and legacy mod shapes", () => {
    const score = {
      mods: ["HD", { acronym: "DT" }, {}],
    } as Score;
    expect(scoreMods(score)).toEqual(["HD", "DT"]);
  });

  it("reads the message from structured command errors", () => {
    expect(errorMessage({ code: "SIMILARITY_INDEX_INVALID", message: "Index unavailable" }))
      .toBe("Index unavailable");
  });
});
