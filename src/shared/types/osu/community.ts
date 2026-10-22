// 从 types/osu.ts 按域拆分；types/osu.ts 是对外的 barrel，消费方导入路径不变。
import type { Ruleset } from "./common";

export interface CommunityPage<T> {
  items: T[];
  next_cursor: string | null;
  server_time: string;
}
export type CommunityActivity = "mp" | "duel" | "ranked" | "practice" | "other";
export type CommunityPlatform = "osu" | "romai" | "vash" | "osu_rl" | "other";
export type LobbyStatus = "upcoming" | "ongoing" | "ended" | "closed";
export type TournamentStatus = "registering" | "upcoming" | "ongoing" | "ended";
export interface CommunityQuery {
  q?: string;
  ruleset?: Ruleset;
  activity_type?: CommunityActivity;
  platform?: CommunityPlatform;
  status?: "active" | "all" | LobbyStatus | TournamentStatus;
  cursor?: string;
  limit?: number;
}
export interface CommunityLobbyInput {
  title: string;
  description: string;
  ruleset: Ruleset;
  activity_type: CommunityActivity;
  platform: CommunityPlatform | null;
  starts_at: string;
  ends_at: string;
}
export interface CommunityLobby extends CommunityLobbyInput {
  id: string;
  owner_user_id: string;
  osu_user_id: string;
  osu_username: string;
  closed_at: string | null;
  status: LobbyStatus;
  created_at: string;
  updated_at: string;
}
export interface CommunityTournament {
  id: string;
  title: string;
  organizer: string;
  ruleset: Ruleset;
  summary: string;
  description: string;
  requirements: string;
  poster_url: string | null;
  registration_url: string | null;
  registration_starts_at: string | null;
  registration_ends_at: string | null;
  starts_at: string | null;
  ends_at: string | null;
  status: TournamentStatus;
  created_at: string;
  updated_at: string;
}
