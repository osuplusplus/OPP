import { useLocalBeatmapBackground } from "./api";
import type { OsuClient } from "../../shared/types/osu";
import { useArtworkPalette } from "../../shared/lib/stageArtwork";

export function useStageArtwork(client: OsuClient, resourceId: string | null) {
  const query = useLocalBeatmapBackground(client, resourceId, "stage");
  return useArtworkPalette(query.isError ? null : query.data ?? null, Boolean(resourceId && query.isPending));
}

