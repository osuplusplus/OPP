export function beatmapPreviewRoute(beatmapId: number | null) {
  return beatmapId && beatmapId > 0 ? `/tools/beatmaps?preview_bid=${beatmapId}` : null;
}

export const toolRoutes = {
  game: "/tools/game",
  beatmaps: "/tools/beatmaps",
  system: "/tools/system",
  live: "/tools/live",
} as const;
