import { describe, expect, it } from "vitest";
import { resultLayout, visibleResultRows } from "./resultLayout";

describe("result row layout", () => {
  it("lays out measured card bodies with a gap below the tallest card", () => {
    const layout = resultLayout([1, 2, 3, 4, 5], 2, new Map([[1, 210], [2, 360], [5, 200]]));
    expect(layout.rows).toEqual([{ top: 0, height: 360 }, { top: 372, height: 160 }, { top: 544, height: 200 }]);
    expect(layout.totalHeight).toBe(744);
    expect(resultLayout([1, 2, 3], 1, new Map()).rows[2].top).toBe(344);
  });
  it("finds visible variable-height rows and renders only a bounded buffer", () => {
    const layout = resultLayout(Array.from({ length: 100 }, (_, i) => i), 1, new Map([[10, 400]]));
    const visible = visibleResultRows(layout.rows, 2000, 300);
    expect(visible).toEqual({ start: 7, end: 15 });
    expect(resultLayout([], 1, new Map()).totalHeight).toBe(0);
  });
});
