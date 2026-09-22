import type { TournamentPool, TournamentPoolRef } from "../../shared/types/osu";
import type { BeatmapDownloadSelection } from "../online-beatmaps/api";

const stages: Record<TournamentPoolRef["category"], string> = {
  qualification: "资格赛", ro16: "十六强", quarterfinals: "四分之一决赛",
  semifinals: "半决赛", finals: "决赛", grandfinals: "总决赛",
};

export const poolTitle = (reference: TournamentPoolRef) => `Rino ${reference.season.toUpperCase()} ${stages[reference.category]}`;

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
