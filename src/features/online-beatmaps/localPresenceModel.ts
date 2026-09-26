import type { LocalBeatmapPresence, OnlineBeatmapset } from "../../shared/types/osu";

export type PresenceMap = ReadonlyMap<number, LocalBeatmapPresence>;
export function summarizeLocalPresence(set: OnlineBeatmapset, presence: PresenceMap) {
  const ids = [...new Set((set.beatmaps ?? []).map((map) => map.id))];
  const present = ids.filter((id) => presence.get(id)?.status === "present").length;
  if (ids.length && present === ids.length) return { state: "present", label: "本地已有" };
  if (present) return { state: "partial", label: `部分已有 ${present}/${ids.length}` };
  if (!ids.length || ids.some((id) => !presence.has(id) || presence.get(id)?.status === "unknown")) return { state: "unknown", label: "本地状态未知" };
  return { state: "missing", label: "本地未收录" };
}
