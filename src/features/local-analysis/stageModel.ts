import type { BeatmapQuery, LocalBeatmapSetSummary, OsuClient, Ruleset } from "../../shared/types/osu";
export { extractStageHue } from "../../shared/lib/stageArtwork";

export const rangeFields = [
  ["min_stars", "最低星数", 1], ["max_stars", "最高星数", 1],
  ["min_bpm", "最低 BPM", 1], ["max_bpm", "最高 BPM", 1],
  ["min_length_ms", "最短秒数", 1000], ["max_length_ms", "最长秒数", 1000],
  ["min_objects", "最少物件", 1], ["max_objects", "最多物件", 1],
  ["min_ar", "最低 AR", 1], ["max_ar", "最高 AR", 1],
  ["min_cs", "最低 CS", 1], ["max_cs", "最高 CS", 1],
  ["min_od", "最低 OD", 1], ["max_od", "最高 OD", 1],
] as const;

export function createStageQuery(client: OsuClient, ruleset: Ruleset): BeatmapQuery {
  return {
    client, rulesets: [ruleset], search: "", sort: "title", direction: "asc", offset: 0, limit: 20,
    min_stars: null, max_stars: null, min_bpm: null, max_bpm: null,
    min_length_ms: null, max_length_ms: null, min_objects: null, max_objects: null,
    min_ar: null, max_ar: null, min_cs: null, max_cs: null, min_od: null, max_od: null, submitted: null,
  };
}

export function sortedDifficulties(set: LocalBeatmapSetSummary) {
  return [...set.difficulties].sort((a, b) => (a.stars ?? Infinity) - (b.stars ?? Infinity)
    || a.difficulty_name.localeCompare(b.difficulty_name) || a.resource.resource_id.localeCompare(b.resource.resource_id));
}

export function initialDifficulty(set: LocalBeatmapSetSummary, matched: ReadonlySet<string>) {
  const sorted = sortedDifficulties(set);
  return sorted.find((difficulty) => matched.has(difficulty.resource.resource_id)) ?? sorted[0];
}

export function durationLabel(seconds: number) {
  const safe = Number.isFinite(seconds) ? Math.max(0, Math.floor(seconds)) : 0;
  return `${Math.floor(safe / 60)}:${String(safe % 60).padStart(2, "0")}`;
}
