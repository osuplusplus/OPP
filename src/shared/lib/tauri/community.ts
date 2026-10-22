import type { CommunityLobby, CommunityLobbyInput, CommunityPage, CommunityQuery, CommunityTournament } from "../../types/osu";
import { call } from "./invoke";

// community 域的 Tauri 命令适配（从 lib/tauri.ts 拆分，desktopApi 组合导出不变）。
export const communityApi = {
  listCommunityTournaments: (query: CommunityQuery) => call<CommunityPage<CommunityTournament>>("list_community_tournaments", { query }),

  getCommunityTournament: (id: string) => call<CommunityTournament>("get_community_tournament", { id }),

  listCommunityLobbies: (query: CommunityQuery, mine = false) => call<CommunityPage<CommunityLobby>>("list_community_lobbies", { query, mine }),

  getCommunityLobby: (id: string) => call<CommunityLobby>("get_community_lobby", { id }),

  saveCommunityLobby: (input: CommunityLobbyInput, id: string | null = null) => call<CommunityLobby>("save_community_lobby", { id, input }),

  closeCommunityLobby: (id: string) => call<CommunityLobby>("close_community_lobby", { id }),

  deleteCommunityLobby: (id: string) => call<void>("delete_community_lobby", { id }),
};
