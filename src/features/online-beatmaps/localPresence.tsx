import type { OnlineBeatmapset } from "../../shared/types/osu";
import { summarizeLocalPresence, type PresenceMap } from "./localPresenceModel";
import { OnlineStatusBadge } from "./OnlineStatusBadge";

export function LocalSetBadge({ set, presence }: { set: OnlineBeatmapset; presence?: PresenceMap }) {
  if (!presence) return null;
  const { state, label } = summarizeLocalPresence(set, presence);
  return <OnlineStatusBadge status={`local_${state}`} label={label} title="依据最近一次本地扫描按谱面 ID 匹配，文件版本可能不同。未扫描、扫描中或来源不可访问时，缺失状态为未知。" />;
}

export function LocalDifficultyBadge({ id, presence }: { id: number; presence?: PresenceMap }) {
  if (!presence) return null;
  const item = presence.get(id);
  if (item?.status !== "present") return null;
  return <OnlineStatusBadge status="local_present" title={`本地已有 · ${item.clients.map((client) => client === "stable" ? "Stable" : "lazer").join(" / ")} · 按 ID 匹配`} />;
}
