import type { CommunityLobby, CommunityLobbyInput, CommunityQuery, LobbyStatus, Ruleset } from "../../shared/types/osu";

export const activityLabels = { mp: "约 MP", duel: "约战", ranked: "排位找搭子", practice: "一起练图", other: "其他" } as const;
export const platformLabels = { osu: "普通 osu!", romai: "RomAI", vash: "Vash Esports", osu_rl: "osu! ranked lobbies", other: "其他平台" } as const;
export const rulesetLabels = { osu: "osu!standard", taiko: "osu!taiko", fruits: "osu!catch", mania: "osu!mania" } as const;
export const statusLabels = { registering: "报名中", upcoming: "即将开始", ongoing: "进行中", ended: "已结束", closed: "提前结束" } as const;
export function activityTime(value: string) {
  return new Date(value).toLocaleString(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit", year: "numeric" });
}
export function tournamentTimeRange(start: string | null, end: string | null) {
  if (!start && !end) return "待公布";
  return `${start ? activityTime(start) : "待公布"} — ${end ? activityTime(end) : "待公布"}`;
}
export function communityError(error: unknown) {
  const value = error as { message?: string; code?: string } | null;
  if (value?.code === "INSUFFICIENT_SCOPE") return "当前身份没有操作权限。";
  if (value?.code === "COMMUNITY_NOT_FOUND") return "这条信息已删除或被管理员下架。";
  if (value?.code === "LOBBY_ENDED") return "活动已结束，请刷新列表。";
  return value?.message ?? "暂时无法完成操作，请重试。";
}
export function localDateInput(value: string) {
  const date = new Date(value);
  const adjusted = new Date(date.getTime() - date.getTimezoneOffset() * 60_000);
  return adjusted.toISOString().slice(0, 16);
}
export type LobbyDraft = Omit<CommunityLobbyInput, "starts_at" | "ends_at"> & { starts_at: string; ends_at: string };
export function lobbyDraft(lobby?: CommunityLobby, ruleset: Ruleset = "osu"): LobbyDraft {
  const now = new Date();
  return { title: lobby?.title ?? "", description: lobby?.description ?? "", activity_type: lobby?.activity_type ?? "mp", platform: lobby?.platform ?? null, ruleset: lobby?.ruleset ?? ruleset,
    starts_at: localDateInput(lobby?.starts_at ?? now.toISOString()), ends_at: localDateInput(lobby?.ends_at ?? new Date(now.getTime() + 2 * 3600_000).toISOString()) };
}
export function draftInput(draft: LobbyDraft, now = Date.now()): CommunityLobbyInput {
  const start = new Date(draft.starts_at); const end = new Date(draft.ends_at);
  if (!draft.title.trim() || !draft.description.trim()) throw new Error("请填写标题和活动描述。");
  if (!Number.isFinite(start.getTime()) || !Number.isFinite(end.getTime()) || end <= start || end.getTime() <= now) throw new Error("结束时间必须晚于开始时间和当前时间。");
  return { ...draft, title: draft.title.trim(), description: draft.description.trim(), starts_at: start.toISOString(), ends_at: end.toISOString() };
}
export function lobbyState(lobby: CommunityLobby, now: Date): LobbyStatus {
  if (lobby.closed_at) return "closed";
  const clock = now.getTime();
  if (new Date(lobby.ends_at).getTime() <= clock) return "ended";
  return new Date(lobby.starts_at).getTime() <= clock ? "ongoing" : "upcoming";
}
export function queryFromParams(params: URLSearchParams): CommunityQuery {
  const query: CommunityQuery = {};
  const q = params.get("q"); if (q) query.q = q;
  const mode = params.get("ruleset"); if (mode && mode in rulesetLabels) query.ruleset = mode as Ruleset;
  const type = params.get("activity_type"); if (type && type in activityLabels) query.activity_type = type as CommunityQuery["activity_type"];
  const platform = params.get("platform"); if (platform && platform in platformLabels) query.platform = platform as CommunityQuery["platform"];
  const status = params.get("status"); if (status && (status === "active" || status === "all" || status in statusLabels)) query.status = status as CommunityQuery["status"];
  return query;
}
export async function copyCommunityText(value: string): Promise<string> {
  try { await navigator.clipboard.writeText(value); return "已复制"; }
  catch { return `复制失败，请手动复制：${value}`; }
}
