export function beatmapPreviewRoute(beatmapId: number | null) {
  return beatmapId && beatmapId > 0 ? `/tools/beatmaps?preview_bid=${beatmapId}` : null;
}
