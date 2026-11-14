import { describe, expect, it } from "vitest";
import type { RomaiPoolSummary } from "../../shared/types/osu";
import {
  beatmapCoverUrl,
  collectorAvatar,
  collectorSearchParams,
  parseHubInput,
  romaiFilterPools,
  romaiModChips,
  romaiSlotLabel,
  romaiTierCounts,
  romaiTierMeta,
  starBinColor,
} from "./model";

describe("Hub input routing", () => {
  it("normalizes codes while retaining ambiguous bare-code search", () => {
    expect(parseHubInput(" bph-7k3n9a ")).toEqual({ kind: "code", id: "7K3N9A", bare: false });
    expect(parseHubInput("7k3n9a")).toEqual({ kind: "code", id: "7K3N9A", bare: true });
    expect(parseHubInput("BPH-O0I1LL")).toEqual({ kind: "invalid" });
    expect(parseHubInput("tech practice")).toEqual({ kind: "search", query: "tech practice" });
    expect(parseHubInput("  ")).toEqual({ kind: "search", query: "" });
  });
});

describe("osu!Collector search mapping", () => {
  it("maps UI sort keys to the search endpoint fields", () => {
    expect(collectorSearchParams({ search: "  ", sort: "relevance" })).toEqual({ search: null, sortBy: "_text_match", orderBy: "desc" });
    expect(collectorSearchParams({ search: "nm1", sort: "relevance" })).toEqual({ search: "nm1", sortBy: "_text_match", orderBy: "desc" });
    expect(collectorSearchParams({ search: "alt", sort: "fruits" })).toEqual({ search: "alt", sortBy: "catchCount", orderBy: "desc" });
    expect(collectorSearchParams({ search: "tech", sort: "favourites" })).toEqual({ search: "tech", sortBy: "favourites", orderBy: "desc" });
    expect(collectorSearchParams({ search: "", sort: "recent" })).toEqual({ search: null, sortBy: "dateUploaded", orderBy: "desc" });
  });
  it("colors star bins on the osu!Collector scale and clamps the range", () => {
    expect(starBinColor(1)).toBe("#6EFF79");
    expect(starBinColor(5)).toBe("#FF4E6F");
    expect(starBinColor(7)).toBe("#3B38B2");
    expect(starBinColor(8)).toBe("var(--hub-star-high)");
    expect(starBinColor(99)).toBe(starBinColor(10));
    expect(starBinColor(0)).toBe(starBinColor(1));
  });
  it("only builds uploader avatars for real user ids", () => {
    expect(collectorAvatar(9044476)).toBe("https://a.ppy.sh/9044476");
    expect(collectorAvatar(null)).toBeNull();
    expect(collectorAvatar(0)).toBeNull();
  });
});

describe("RomAI pool browsing model", () => {
  const pool = (over: Partial<RomaiPoolSummary>): RomaiPoolSummary => ({
    id: "6649b30ad5efb507e7b8d7b5", name: "Sample Cup (Finals)", elo: 1676, tier: "atomos",
    map_count: 10, counts: { nm: 6, hd: 3, hr: 3, dt: 3, fm: 0, tb: 1 }, covers: [1616036],
    ...over,
  });
  const pools = [
    pool({ id: "a", name: "Alpha Cup", elo: 2100, tier: "quantum" }),
    pool({ id: "b", name: "Beta Cup", elo: 983, tier: "candidate", counts: { nm: 3, hd: 0, hr: 0, dt: 0, fm: 2, tb: 0 } }),
    pool({ id: "c", name: "Gamma Cup", elo: 1676, tier: "atomos" }),
  ];
  it("labels slots with TB left unnumbered", () => {
    expect(romaiSlotLabel("NM", 1)).toBe("NM1");
    expect(romaiSlotLabel("HD", 2)).toBe("HD2");
    expect(romaiSlotLabel("TB", 1)).toBe("TB");
  });
  it("renders mod chips for non-zero groups with a bare TB", () => {
    expect(romaiModChips({ nm: 6, hd: 3, hr: 3, dt: 3, fm: 0, tb: 1 }).map((chip) => `${chip.count ?? ""}${chip.label}`))
      .toEqual(["6NM", "3HD", "3HR", "3DT", "TB"]);
  });
  it("filters by tier and keyword, then sorts by the selected order", () => {
    expect(romaiFilterPools(pools, { search: "", tier: "atomos", sort: "elo-desc" }).map((item) => item.id)).toEqual(["c"]);
    expect(romaiFilterPools(pools, { search: "cup", tier: null, sort: "elo-asc" }).map((item) => item.id)).toEqual(["b", "c", "a"]);
    expect(romaiFilterPools(pools, { search: "ALPHA", tier: null, sort: "name" }).map((item) => item.id)).toEqual(["a"]);
  });
  it("counts pools per tier for the filter pills", () => {
    expect(romaiTierCounts(pools).get("candidate")).toBe(1);
    expect(romaiTierCounts(pools).get("quantum")).toBe(1);
  });
  it("falls back to the lowest tier meta and builds cover URLs", () => {
    expect(romaiTierMeta("atomos").label).toBe("Atomos");
    expect(romaiTierMeta("candidate").floor).toBe(0);
    expect(beatmapCoverUrl(1616036)).toBe("https://assets.ppy.sh/beatmaps/1616036/covers/cover.jpg");
  });
});
