import type { CollectionEntry, CollectionPoolSnapshot, TournamentPool, TournamentPoolRef } from "../../shared/types/osu";
import type { BeatmapDownloadSelection } from "../online-beatmaps/api";

const stages: Record<string, string> = {
  qualification: "资格赛", ro16: "十六强", quarterfinals: "四分之一决赛",
  semifinals: "半决赛", finals: "决赛", grandfinals: "总决赛",
};

export const poolTitle = (reference: TournamentPoolRef) => reference.provider === "opp" ? "OPP 图池" : `ASC 星域杯 ${reference.season.toUpperCase()} ${stages[reference.category]}`;

export function poolDownloads(pool: TournamentPool | undefined) {
  const sets = new Map<number, BeatmapDownloadSelection>();
  const unavailable: number[] = [];
  for (const entry of pool?.entries ?? []) {
    const map = entry.beatmap;
    if (!map || map.download_disabled) { unavailable.push(entry.beatmap_id); continue; }
    let set = sets.get(map.beatmapset_id);
    if (!set) {
      set = { id: map.beatmapset_id, title: map.title, artist: map.artist, creator: map.creator, status: "unknown", beatmaps: [], allow_extra_difficulties: true };
      sets.set(set.id, set);
    }
    if (!set.beatmaps!.some((beatmap) => beatmap.id === entry.beatmap_id)) set.beatmaps!.push({ id: entry.beatmap_id });
  }
  return { items: [...sets.values()], unavailable: [...new Set(unavailable)] };
}

export function savedPoolDownloads(entries: CollectionEntry[], pool: CollectionPoolSnapshot) {
  return poolDownloads({ reference: pool.reference, title: pool.title ?? "", entries: entries.filter((entry) => !!entry.beatmap_id).map((entry) => ({
    beatmap_id: entry.beatmap_id!, selection_type: "", position: 0, selected_by: null, selected_by_name: null,
    comment: "", is_custom: false, is_original: false, resolution_error: null,
    beatmap: entry.beatmapset_id ? { beatmapset_id: entry.beatmapset_id, title: entry.title, artist: entry.artist,
      creator: entry.creator, difficulty_name: entry.difficulty_name, checksum: entry.checksum,
      download_disabled: pool.slots.some((slot) => slot.beatmap_id === entry.beatmap_id && slot.download_disabled),
    } : null,
  })) });
}
