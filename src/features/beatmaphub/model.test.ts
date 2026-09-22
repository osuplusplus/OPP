import { describe, expect, it } from "vitest";
import { packCover, parseHubInput } from "./model";

describe("Hub input routing", () => {
  it("normalizes codes while retaining ambiguous bare-code search", () => {
    expect(parseHubInput(" bph-7k3n9a ")).toEqual({ kind: "code", id: "7K3N9A", bare: false });
    expect(parseHubInput("7k3n9a")).toEqual({ kind: "code", id: "7K3N9A", bare: true });
    expect(parseHubInput("BPH-O0I1LL")).toEqual({ kind: "invalid" });
    expect(parseHubInput("tech practice")).toEqual({ kind: "search", query: "tech practice" });
    expect(parseHubInput("  ")).toEqual({ kind: "search", query: "" });
  });
  it("only requests a cover for a valid set ID", () => {
    expect(packCover([-1, 0, 123])).toContain("/123/");
    expect(packCover([])).toBeNull();
  });
});
