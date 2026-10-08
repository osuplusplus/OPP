import type { CommunityLobby, CommunityLobbyInput, CommunityQuery, CommunityTournament } from "../types/osu";

// In-memory browser preview only. Desktop calls always use the VPS.
const hours = (offset: number) => new Date(Date.now() + offset * 3600_000).toISOString();
const lobbies: CommunityLobby[] = [
  { id: "00000000-0000-4000-8000-000000000001", title: "今晚来一场轻松的 MP", description: "4–6★，轮流选图，什么风格都可以。\n不用纠结成绩，一起发现好图。新人也欢迎！", activity_type: "mp", platform: "osu", ruleset: "osu", osu_user_id: "10001", osu_username: "Preview User", owner_user_id: "preview-owner", starts_at: hours(1), ends_at: hours(3), closed_at: null, status: "upcoming", created_at: hours(-1), updated_at: hours(-1) },
  { id: "00000000-0000-4000-8000-000000000002", title: "RomAI 双排找个搭子", description: "想一起打 2v2 排位，认真打也可以，主要是享受比赛。\n水平差不多就好，感兴趣可以 osu! 私信我。", activity_type: "ranked", platform: "romai", ruleset: "osu", osu_user_id: "10002", osu_username: "Map Explorer", owner_user_id: "preview-other", starts_at: hours(2), ends_at: hours(5), closed_at: null, status: "upcoming", created_at: hours(-2), updated_at: hours(-2) },
  { id: "00000000-0000-4000-8000-000000000003", title: "4K 一起练耐力", description: "一起练长图、交流手法。\n目前在练 4–5★，带上你的推荐图！", activity_type: "practice", platform: null, ruleset: "mania", osu_user_id: "10003", osu_username: "Key Garden", owner_user_id: "preview-other", starts_at: hours(-1), ends_at: hours(2), closed_at: null, status: "ongoing", created_at: hours(-3), updated_at: hours(-3) },
];
const tournaments: CommunityTournament[] = [
  { id: "00000000-0000-4000-8000-000000000101", title: "OPP 秋日社区杯 · 示例赛事", organizer: "OPP 社区", ruleset: "osu", summary: "一场属于社区玩家的友好比赛。现在报名，和熟悉的朋友一起走上比赛舞台。", description: "这是一条浏览器预览示例公告。\n\n比赛采用 1v1 赛制，详细规则和图池请查看主办方的报名页面。", requirements: "欢迎符合主办方参赛要求的玩家报名。", poster_url: null, registration_url: "https://osu.ppy.sh/community/forums", registration_starts_at: hours(-24), registration_ends_at: hours(72), starts_at: hours(168), ends_at: hours(192), status: "registering", created_at: hours(-2), updated_at: hours(-2) },
  { id: "00000000-0000-4000-8000-000000000102", title: "4K 新人交流赛 · 示例赛事", organizer: "Key Garden", ruleset: "mania", summary: "从练图到赛场，和其他玩家一起体验比赛的乐趣。", description: "浏览器预览示例。\n模式为 osu!mania 4K，报名详情以主办方公布内容为准。", requirements: "4K 玩家", poster_url: null, registration_url: null, registration_starts_at: hours(24), registration_ends_at: hours(120), starts_at: hours(144), ends_at: hours(168), status: "upcoming", created_at: hours(-3), updated_at: hours(-3) },
];
let connected = false;
export function communityPreview(command: string, args?: Record<string, unknown>): unknown {
  if (command === "get_auth_status") return { credentials_configured: true, connected: true, client_id: "preview", callback_url: "http://127.0.0.1:1420/preview", user_id: 10001, username: "Preview User" };
  const auth = { has_identity: connected, connected, user_id: connected ? "10001" : null, display_name: connected ? "Preview User" : null, public_key: null, device_id: connected ? "preview-device" : null, device_name: "Preview PC", expires_at: connected ? hours(1) : null };
  if (command === "get_beatmaphub_auth_status") return auth;
  if (["login_beatmaphub", "bootstrap_beatmaphub", "reconnect_beatmaphub"].includes(command)) { connected = true; return { ...auth, has_identity: true, connected: true, user_id: "10001", display_name: "Preview User", device_id: "preview-device" }; }
  if (command === "get_beatmaphub_profile") return { user: { id: "preview-owner", display_name: "Preview User" }, current_device_id: "preview-device", devices: [] };
  if (command === "logout_beatmaphub") { connected = false; return null; }
  const id = args?.id as string; const query = (args?.query ?? {}) as CommunityQuery;
  if (command.startsWith("list_community_")) {
    const source = command === "list_community_lobbies" ? lobbies : tournaments;
    const filtered = source.filter((item) => (!args?.mine || ("owner_user_id" in item && item.owner_user_id === "preview-owner")) && (!query.ruleset || item.ruleset === query.ruleset) && (!query.q || `${item.title} ${item.description} ${"osu_username" in item ? item.osu_username : item.organizer}`.toLowerCase().includes(query.q.toLowerCase())) && (!query.activity_type || ("activity_type" in item && item.activity_type === query.activity_type)) && (!query.platform || ("platform" in item && item.platform === query.platform)) && (!query.status || query.status === "all" || (query.status === "active" ? item.status !== "ended" && item.status !== "closed" : item.status === query.status)));
    return { items: filtered, next_cursor: null, server_time: new Date().toISOString() };
  }
  if (command === "get_community_lobby" || command === "get_community_tournament") {
    const item = (command === "get_community_lobby" ? lobbies : tournaments).find((item) => item.id === id);
    if (!item) throw { code: "COMMUNITY_NOT_FOUND", message: "内容已删除" }; return item;
  }
  if (command === "save_community_lobby") {
    const input = args?.input as CommunityLobbyInput; const existing = lobbies.find((item) => item.id === id);
    const item: CommunityLobby = { ...input, id: id || crypto.randomUUID(), owner_user_id: "preview-owner", osu_user_id: "10001", osu_username: "Preview User", created_at: existing?.created_at ?? hours(0), updated_at: hours(0), closed_at: null, status: new Date(input.starts_at).getTime() <= Date.now() ? "ongoing" : "upcoming" };
    if (existing) lobbies.splice(lobbies.indexOf(existing), 1, item); else lobbies.unshift(item); return item;
  }
  if (command === "close_community_lobby") { const item = lobbies.find((item) => item.id === id); if (item) { item.closed_at = hours(0); item.status = "closed"; } return item; }
  if (command === "delete_community_lobby") { const index = lobbies.findIndex((item) => item.id === id); if (index >= 0) lobbies.splice(index, 1); return null; }
  return undefined;
}
