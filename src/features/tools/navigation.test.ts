import { describe, expect, it } from "vitest";

import { beatmapPreviewRoute } from "./navigation";

describe("beatmap preview navigation", () => {
  it("builds a prefilled tools route for published local maps", () => {
    expect(beatmapPreviewRoute(738063)).toBe("/tools/beatmaps?preview_bid=738063");
  });

  it("does not expose a route for local-only maps", () => {
    expect(beatmapPreviewRoute(null)).toBeNull();
    expect(beatmapPreviewRoute(0)).toBeNull();
  });
});
