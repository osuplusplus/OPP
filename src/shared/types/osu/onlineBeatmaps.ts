// 从 types/osu.ts 按域拆分；types/osu.ts 是对外的 barrel，消费方导入路径不变。
import type { Ruleset } from "./common";
import type { BeatmapDownloadProvider } from "./localResources";

export interface OnlineBeatmapSearchQuery {
  query: string;
  ruleset: Ruleset | null;
  status: string;
  genre: number | null;
  language: number | null;
  extras: Array<"video" | "storyboard">;
  include_nsfw: boolean;
  sort: string;
  artist: string;
  title: string;
  title_unicode: string;
  source: string;
  mapper: string;
  difficulty: string;
  tags: string;
  ranked_from: string;
  ranked_to: string;
  submitted_from: string;
  submitted_to: string;
  updated_from: string;
  updated_to: string;
  favourites_min: number | null;
  favourites_max: number | null;
  stars_min: number | null;
  stars_max: number | null;
  bpm_min: number | null;
  bpm_max: number | null;
  length_min: number | null;
  length_max: number | null;
  ar_min: number | null;
  ar_max: number | null;
  cs_min: number | null;
  cs_max: number | null;
  od_min: number | null;
  od_max: number | null;
  hp_min: number | null;
  hp_max: number | null;
  keys_min: number | null;
  keys_max: number | null;
  cursor_string: string | null;
  content_filter: string;
  grade: string;
  played: string;
}

export type BeatmapSource = "official" | "nerinyan" | "catboy";

export interface BeatmapSourceStatus {
  id: BeatmapSource;
  label: string;
  online: boolean;
  supports_search: boolean;
  supports_metadata: boolean;
  supports_osu_download: boolean;
  supports_osz_download: boolean;
  retry_after_seconds?: number | null;
  message?: string | null;
}

export interface BeatmapCalculationRequest {
  beatmap_id: number;
  mods: string[];
  accuracy?: number;
  misses?: number;
  combo?: number;
  n300?: number;
  n100?: number;
  n50?: number;
}

export interface BeatmapCalculationResult {
  beatmap_id: number;
  mods: string[];
  mode: string;
  stars: number;
  pp: number;
  max_pp: number;
  max_combo: number;
  calculation_engine: string;
  calculated_at: string;
  source: BeatmapSource;
  star_algorithm: string;
  star_algorithm_date: string;
  performance_algorithm: string;
  performance_algorithm_date: string;
}

export interface OnlineBeatmapCover {
  cover?: string;
  "cover@2x"?: string;
  card?: string;
  "card@2x"?: string;
  list?: string;
  "list@2x"?: string;
  slimcover?: string;
  "slimcover@2x"?: string;
}

export interface OnlineBeatmap {
  id: number;
  beatmapset_id: number;
  max_combo?: number;
  beatmapset?: OnlineBeatmapset;
  difficulty_rating: number;
  mode: Ruleset;
  mode_int?: number;
  status: string;
  total_length: number;
  hit_length?: number;
  version: string;
  accuracy?: number;
  ar?: number;
  bpm?: number;
  convert?: boolean;
  count_circles?: number;
  count_sliders?: number;
  count_spinners?: number;
  cs?: number;
  drain?: number;
  passcount?: number;
  playcount?: number;
  last_updated?: string;
  url?: string;
  checksum?: string;
}

export interface OnlineBeatmapset {
  id: number;
  user_id?: number;
  artist: string;
  artist_unicode?: string;
  title: string;
  title_unicode?: string;
  creator: string;
  source?: string;
  status: string;
  ranked_date?: string | null;
  submitted_date?: string | null;
  last_updated?: string | null;
  bpm?: number;
  favourite_count?: number;
  play_count?: number;
  preview_url?: string;
  nsfw?: boolean;
  video?: boolean;
  storyboard?: boolean;
  tags?: string;
  covers?: OnlineBeatmapCover;
  beatmaps?: OnlineBeatmap[];
  genre?: { id?: number; name?: string };
  language?: { id?: number; name?: string };
  ratings?: number[];
  availability?: {
    download_disabled?: boolean;
    more_information?: string | null;
  };
  description?: { description?: string | null };
}

export interface OnlineBeatmapSearchResponse {
  beatmapsets: OnlineBeatmapset[];
  cursor_string?: string | null;
  total?: number;
}

export interface CollectedBeatmapsets {
  items: OnlineBeatmapset[];
  available_total: number | null;
  truncated: boolean;
}

export interface BeatmapDownloadItem {
  beatmapset_id: number;
  artist: string;
  title: string;
  expected_beatmap_ids?: number[];
  allow_extra_difficulties?: boolean;
}

export interface BeatmapDownloadRequest {
  destination: string;
  items: BeatmapDownloadItem[];
  provider: BeatmapDownloadProvider | "none";
  overwrite: boolean;
  include_video: boolean;
  open_after_download?: boolean;
}

export interface BeatmapDownloadFailure {
  beatmapset_id: number;
  title: string;
  message: string;
}

export interface BeatmapDownloadResult {
  destination: string;
  total: number;
  completed: number;
  skipped: number;
  failed: number;
  cancelled: boolean;
  failures: BeatmapDownloadFailure[];
  completed_paths?: string[];
  succeeded_beatmapset_ids?: number[];
}

export interface BeatmapDownloadProgress {
  phase:
    | "started"
    | "downloading"
    | "completed"
    | "skipped"
    | "failed"
    | "finished"
    | "cancelled";
  total: number;
  processed: number;
  completed: number;
  skipped: number;
  failed: number;
  current_beatmapset_id: number | null;
  current_title: string | null;
  message: string | null;
  downloaded_bytes?: number;
  total_bytes?: number | null;
  bytes_per_second?: number;
  completed_paths?: string[];
  destination?: string;
}
