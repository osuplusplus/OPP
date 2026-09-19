import type { LocalBeatmapSetSummary, LocalBeatmapSummary } from "../../shared/types/osu";

function difficulty(id: string, stars: number, name: string): LocalBeatmapSummary {
  return {
    resource: { resource_id: id, client: "stable", content_hash: `hash-${id}`, logical_path: `Songs/${id}` },
    set_key: "set-1",
    set_grouping_inferred: false,
    beatmap_id: Number(id),
    beatmap_set_id: 456,
    title: "Local Song",
    title_unicode: "",
    artist: "Local Artist",
    artist_unicode: "",
    creator: "Local Mapper",
    difficulty_name: name,
    ruleset: "osu",
    format_version: 14,
    stars,
    max_pp: 300,
    max_combo: 500,
    bpm: 180,
    length_ms: 120_000,
    object_count: 500,
    cs: 4,
    ar: 9,
    od: 8,
    hp: 6,
    average_nps: 4.2,
    peak_nps: 7.1,
    modified_at: null,
    analysis_status: "ready",
  };
}

export const stageSet: LocalBeatmapSetSummary = {
  set_key: "set-1",
  completeness: "complete",
  grouping_inferred: false,
  beatmap_set_id: 456,
  title: "Local Song",
  title_unicode: "",
  artist: "Local Artist",
  artist_unicode: "",
  creators: ["Local Mapper"],
  min_stars: 2.1,
  max_stars: 5.4,
  bpm: 180,
  length_ms: 120_000,
  object_count: 500,
  modified_at: null,
  background_resource_id: "501",
  difficulties: [difficulty("502", 5.4, "Insane"), difficulty("501", 2.1, "Easy")],
};

