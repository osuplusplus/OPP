import type { OnlineBeatmapset } from "../../shared/types/osu";
import { useLocalBeatmapPresence } from "../local-analysis/api";
import { useSettings } from "../settings/api";
import type { PresenceMap } from "./localPresenceModel";

export function useOnlineLocalPresence(sets: OnlineBeatmapset[]): PresenceMap | undefined {
  const settings = useSettings().data;
  const enabled = settings !== undefined && settings.show_local_beatmap_presence !== false;
  const scope = settings?.local_beatmap_presence_scope ?? "all";
  const query = useLocalBeatmapPresence(sets.flatMap((set) => (set.beatmaps ?? []).map((map) => map.id)), scope === "all" ? null : scope, enabled);
  return enabled ? new Map((query.isError ? [] : query.data ?? []).map((item) => [item.beatmap_id, item])) : undefined;
}
