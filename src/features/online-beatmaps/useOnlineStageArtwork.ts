import { useQuery } from "@tanstack/react-query";
import { desktopApi } from "../../shared/lib/tauri";
import { useArtworkPalette } from "../../shared/lib/stageArtwork";
import type { OnlineBeatmapset } from "../../shared/types/osu";

export function useOnlineStageArtwork(set: OnlineBeatmapset | undefined, loadOriginal = true) {
  const beatmapsetId = set?.id ?? null;
  const original = useQuery({
    queryKey: ["online-beatmap-background", beatmapsetId],
    queryFn: () => desktopApi.getOnlineBeatmapBackground(beatmapsetId!),
    enabled: beatmapsetId !== null && loadOriginal,
    staleTime: Infinity,
    gcTime: 5 * 60_000,
    retry: false,
  });
  const fallback = set?.covers?.["cover@2x"] ?? set?.covers?.cover ?? null;
  const source = original.data ?? fallback;
  return useArtworkPalette(source);
}
