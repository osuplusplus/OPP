export const RESULT_CARD_HEIGHT = 160;
export const RESULT_GAP = 12;

/** Virtualize complete grid rows using measured card bodies, independent of difficulty popovers. */
export function resultLayout(ids: number[], columns: number, heights: ReadonlyMap<number, number>) {
  let totalHeight = 0;
  const rows: { top: number; height: number }[] = [];
  for (let index = 0; index < ids.length; index += columns) {
    const height = Math.max(RESULT_CARD_HEIGHT, ...ids.slice(index, index + columns).map((id) => heights.get(id) ?? RESULT_CARD_HEIGHT));
    rows.push({ top: totalHeight, height });
    totalHeight += height + RESULT_GAP;
  }
  return { rows, totalHeight: Math.max(0, totalHeight - RESULT_GAP) };
}

export function visibleResultRows(rows: { top: number; height: number }[], top: number, height: number) {
  const first = rows.findIndex((row) => row.top + row.height >= top);
  const last = rows.findIndex((row) => row.top > top + height);
  return { start: Math.max(0, (first < 0 ? rows.length : first) - 3), end: Math.min(rows.length, (last < 0 ? rows.length : last) + 3) };
}
