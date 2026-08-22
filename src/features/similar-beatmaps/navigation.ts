import type { AnySimilarityResult, OsuClient, Ruleset } from "../../shared/types/osu";

export type SimilarityLaunch =
  | { kind: "beatmap_id"; beatmapId: string; ruleset: Ruleset }
  | { kind: "local_resource"; client: OsuClient; resourceId: string; ruleset: Ruleset };

export function similarityRouteForBeatmap(beatmapId: number, ruleset: Ruleset = "osu") {
  const params = new URLSearchParams({
    source: "beatmap_id",
    value: String(beatmapId),
    ruleset,
  });
  return `/online/similar?${params}`;
}

export function similarityRouteForLocalResource(
  client: OsuClient,
  resourceId: string,
  ruleset: Ruleset = "osu",
) {
  const params = new URLSearchParams({
    source: "local_resource",
    client,
    resource: resourceId,
    ruleset,
  });
  return `/online/similar?${params}`;
}

/**
 * Opens the online beatmap page's detail panel and selects the exact matching
 * difficulty. The beatmap ID identifies the difficulty; the set ID loads its
 * containing beatmapset.
 */
export function onlineBeatmapRouteForSimilarityResult(
  result: Pick<AnySimilarityResult, "beatmapset_id" | "beatmap_id">,
) {
  const params = new URLSearchParams({
    beatmapset: String(result.beatmapset_id),
    beatmap: String(result.beatmap_id),
  });
  return `/online/beatmaps?${params}`;
}

export function parseSimilarityLaunch(searchParams: URLSearchParams): SimilarityLaunch | null {
  const requestedRuleset = searchParams.get("ruleset");
  const ruleset: Ruleset = requestedRuleset === "taiko" || requestedRuleset === "fruits" || requestedRuleset === "mania"
    ? requestedRuleset
    : "osu";
  const source = searchParams.get("source");
  if (source === "beatmap_id") {
    const beatmapId = searchParams.get("value")?.trim();
    return beatmapId && /^\d+$/.test(beatmapId) ? { kind: "beatmap_id", beatmapId, ruleset } : null;
  }
  if (source === "local_resource") {
    const client = searchParams.get("client");
    const resourceId = searchParams.get("resource")?.trim();
    return (client === "stable" || client === "lazer") && resourceId
      ? { kind: "local_resource", client, resourceId, ruleset }
      : null;
  }
  return null;
}
