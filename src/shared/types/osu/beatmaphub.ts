// 从 types/osu.ts 按域拆分；types/osu.ts 是对外的 barrel，消费方导入路径不变。
export interface BeatmapHubAuthStatus {
  has_identity: boolean;
  connected: boolean;
  public_key: string | null;
  user_id: string | null;
  device_id: string | null;
  display_name: string | null;
  device_name: string | null;
  expires_at: string | null;
}

export interface BeatmapHubDevice {
  id: string;
  device_name: string;
  public_key: string;
  created_at: string;
  last_seen_at: string;
  revoked_at: string | null;
}

export interface BeatmapHubProfile {
  user: { id: string; display_name: string };
  current_device_id: string;
  devices: BeatmapHubDevice[];
}

export interface BeatmapHubPack {
  id: string;
  title: string;
  description: string;
  is_private: boolean;
  owner: { id: string; display_name: string };
  beatmapset_ids: number[];
  stars_min?: number | null;
  stars_max?: number | null;
  manifest_hash: string;
  rating: { average: number | null; count: number };
  likes: { count: number };
  comments: { count: number };
  viewer: { rating: number | null; favorited: boolean; liked: boolean; can_edit: boolean } | null;
  created_at: string;
  updated_at: string;
}

export interface OsekaiMedal {
  Medal_ID: number;
  Name?: string;
  Description?: string;
  Instructions?: string;
  Solution?: string;
  Link?: string;
  Date_Released?: string;
  [key: string]: unknown;
}

export interface OsekaiMedalBeatmap {
  Beatmap_ID?: number;
  Beatmapset_ID?: number;
  Title?: string;
  Artist?: string;
  Version?: string;
  Creator?: string;
  Note?: string;
  Song_Title?: string;
  Song_Artist?: string;
  Difficulty_Name?: string;
  [key: string]: unknown;
}

export interface BeatmapHubComment {
  id: string;
  user: { id: string; display_name: string };
  content: string;
  created_at: string;
  updated_at: string;
}

export type BeatmapHubRecommendation = BeatmapHubPack;

export interface BeatmapHubPackPreview {
  pack: BeatmapHubPack;
  locally_available_ids: number[];
  missing_ids: number[];
}

export interface BeatmapHubPublishResult {
  id: string;
  included: number;
  skipped: number;
}

export interface BeatmapHubImportResult {
  folder_id: string;
  imported_sets: number;
  imported_entries: number;
  unresolved_sets: number;
}

export interface BeatmapHubBackupUploadResult {
  count: number;
  skipped: number;
  manifest_hash: string;
  created_at: string;
  updated_at: string;
}

export interface BeatmapHubBackup {
  owner_user_id: string;
  beatmapset_ids: number[];
  count: number;
  manifest_hash: string;
  created_at: string;
  updated_at: string;
  locally_available_count: number;
  missing_count: number;
}

export interface BeatmapHubBackupRestoreResult {
  folder_id: string;
  folder_name: string;
  total_sets: number;
  local_sets: number;
  missing_sets: number;
  entries: number;
}


// —— osu!Collector（PackHub 补充来源，直连不经 PackHub 服务器）——

/** osu!Collector 收藏集合卡片（recent 列表与详情头部共用）。 */
export interface OsuCollectorCollection {
  id: number;
  name: string;
  uploader: string;
  description: string | null;
  beatmap_count: number;
  favourites: number;
  last_modified: string | null;
}

/** recent 列表页；`next_cursor` 透传给下一次请求实现“加载更多”。 */
export interface OsuCollectorListPage {
  collections: OsuCollectorCollection[];
  next_cursor: number | null;
  has_more: boolean;
}

/** 详情预览中的单个难度行，`local` 来自本地索引比对。 */
export interface OsuCollectorBeatmap {
  beatmap_id: number;
  beatmapset_id: number;
  checksum: string | null;
  mode: string;
  difficulty_name: string;
  difficulty_rating: number | null;
  artist: string;
  title: string;
  creator: string;
  local: boolean;
}

/** 集合详情：元数据 + 本地已有/缺失统计 + 首页难度样本。 */
export interface OsuCollectorCollectionDetail {
  collection: OsuCollectorCollection;
  total_sets: number;
  local_sets: number;
  missing_sets: number;
  unsubmitted_beatmaps: number;
  preview: OsuCollectorBeatmap[];
  has_more: boolean;
}

/** `collector-import-progress` 事件载荷。 */
export interface OsuCollectorImportProgress {
  collection_id: number;
  page: number;
  beatmaps: number;
  total_beatmaps: number;
}
