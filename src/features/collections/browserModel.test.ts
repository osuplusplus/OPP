import { expect, it } from "vitest";
import { highlightParts } from "./browserModel";
import { mosaicTiles as collectionMosaic } from "../../shared/lib/artworkMosaic";
import { explicitLocalTarget } from "../local-analysis/navigation";

it("highlights fragments without interpreting search text as regex", () => {
  expect(highlightParts("这张图 [HD] 容易失误", "[hd] 失误").filter((p) => p.matched).map((p) => p.text)).toEqual(["[HD]", "失误"]);
});
it("fills a small collection mosaic with repeated references while limiting distinct images", () => {
  expect(collectionMosaic([])).toEqual([]);
  const tiles = collectionMosaic(["cover-a", "cover-b", "cover-a"]);
  expect(tiles).toHaveLength(24);
  expect(new Set(tiles)).toEqual(new Set(["cover-a", "cover-b"]));
  expect(collectionMosaic(Array.from({ length: 50 }, (_, i) => `cover-${i}`))).toHaveLength(24);
});
it("accepts an exact local target and rejects incomplete or unknown mode links", () => {
  expect(explicitLocalTarget(new URLSearchParams("client=lazer&ruleset=mania&resource=abc&set=xyz"))).toEqual({ client: "lazer", ruleset: "mania", resource_id: "abc", set_key: "xyz" });
  expect(explicitLocalTarget(new URLSearchParams("client=other&ruleset=osu&resource=abc&set=xyz"))).toBeNull();
  expect(explicitLocalTarget(new URLSearchParams("resource=abc"))).toBeNull();
});
