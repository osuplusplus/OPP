// 从 types/osu.ts 按域拆分；types/osu.ts 是对外的 barrel，消费方导入路径不变。
import type { Score } from "./beatmap";
import type { OsuClient, Ruleset } from "./common";

export interface DisconnectResult {
  revoked: boolean;
  warning: string | null;
}

export interface UserLevel {
  current?: number;
  progress?: number;
}

export interface GradeCounts {
  ssh?: number;
  ss?: number;
  sh?: number;
  s?: number;
  a?: number;
}

export interface UserStatistics {
  count_100?: number;
  count_300?: number;
  count_50?: number;
  count_miss?: number;
  grade_counts?: GradeCounts;
  hit_accuracy?: number;
  is_ranked?: boolean;
  level?: UserLevel;
  maximum_combo?: number;
  play_count?: number;
  play_time?: number;
  pp?: number;
  global_rank?: number | null;
  country_rank?: number | null;
  ranked_score?: number;
  replays_watched_by_others?: number;
  total_hits?: number;
  total_score?: number;
  variants?: Array<Record<string, unknown>>;
  [key: string]: unknown;
}

export interface UserCover {
  custom_url?: string | null;
  url?: string;
  id?: number | null;
}

export interface RankHistory {
  mode?: Ruleset;
  data?: number[];
}

export interface MonthlyCount {
  start_date?: string;
  count?: number;
}

export interface ProfilePage {
  html?: string;
  raw?: string;
}

export interface OwnProfile {
  id: number;
  username: string;
  avatar_url: string;
  avatar_data_url?: string | null;
  country_code: string;
  is_active: boolean;
  is_online: boolean;
  is_supporter: boolean;
  is_restricted?: boolean | null;
  last_visit?: string | null;
  playmode?: Ruleset | null;
  profile_colour?: string | null;
  default_group?: string | null;
  cover?: UserCover | null;
  cover_url?: string | null;
  country?: { code?: string; name?: string };
  statistics?: UserStatistics | null;
  statistics_rulesets?: Partial<Record<Ruleset, UserStatistics>> | null;
  rank_history?: RankHistory | null;
  monthly_playcounts?: MonthlyCount[] | null;
  replays_watched_counts?: MonthlyCount[] | null;
  badges?: Array<Record<string, any>> | null;
  groups?: Array<Record<string, any>> | null;
  user_achievements?: Array<Record<string, any>> | null;
  account_history?: Array<Record<string, any>> | null;
  page?: ProfilePage | null;
  join_date?: string;
  location?: string | null;
  interests?: string | null;
  occupation?: string | null;
  website?: string | null;
  discord?: string | null;
  twitter?: string | null;
  title?: string | null;
  previous_usernames?: string[];
  playstyle?: string[];
  post_count?: number;
  follower_count?: number;
  mapping_follower_count?: number;
  beatmap_playcounts_count?: number;
  favourite_beatmapset_count?: number;
  graveyard_beatmapset_count?: number;
  loved_beatmapset_count?: number;
  pending_beatmapset_count?: number;
  ranked_beatmapset_count?: number;
  guest_beatmapset_count?: number;
  nominated_beatmapset_count?: number;
  scores_best_count?: number;
  scores_first_count?: number;
  scores_recent_count?: number;
  kudosu?: { available?: number; total?: number };
  support_level?: number;
  rank_highest?: { rank?: number; updated_at?: string } | null;
  [key: string]: any;
}

export interface CareerStats {
  pp: number | null;
  global_rank: number | null;
  country_rank: number | null;
  ranked_score: number | null;
  total_score: number | null;
  hit_accuracy: number | null;
  play_count: number | null;
  play_time: number | null;
  total_hits: number | null;
  maximum_combo: number | null;
  level: number | null;
  level_progress: number | null;
}

export interface CareerCalendarDay {
  date: string;
  status: "captured" | "unavailable" | "partial" | "interpolated" | "missing" | string;
  captured_at: string | null;
  stats: CareerStats | null;
  error: string | null;
  has_diff: boolean;
  added_scores: number;
  removed_scores: number;
  changed_scores: number;
  added_medals: number;
  added_replays: number;
  added_screenshots: number;
}

export interface CareerCalendar {
  ruleset: Ruleset;
  start_date: string;
  end_date: string;
  days: CareerCalendarDay[];
}

export interface CareerScoreDiff {
  kind: "added" | "removed" | "changed" | string;
  key: string;
  before_position: number | null;
  after_position: number | null;
  score: Score;
}

export interface CareerEventItem {
  kind: string;
  client: OsuClient | null;
  name: string;
  path: string | null;
  size: number | null;
  modified_at: string | null;
  payload: Record<string, any> | null;
}

export interface CareerDayDetail {
  ruleset: Ruleset;
  date: string;
  status: string;
  captured_at: string | null;
  stats: CareerStats | null;
  previous_stats: CareerStats | null;
  error: string | null;
  score_diffs: CareerScoreDiff[];
  medal_events: CareerEventItem[];
  media_events: CareerEventItem[];
}

export interface CareerStatus {
  configured: boolean;
  path: string | null;
  snapshot_count: number;
  latest_date: string | null;
  last_error: string | null;
}

export interface CareerCaptureResult {
  local_date: string;
  status: string;
  created: boolean;
  fetched_at: string | null;
  profile_available: boolean;
  scores_available: boolean;
  media_available: boolean;
  message: string;
}
