import type {
  OnlineBeatmap,
  OnlineBeatmapSearchQuery,
  Ruleset,
} from "../../shared/types/osu";

export const statusOptions = [
  { value: "any", label: "全部状态" },
  { value: "leaderboard", label: "有排行榜" },
  { value: "ranked", label: "Ranked" },
  { value: "qualified", label: "Qualified" },
  { value: "loved", label: "Loved" },
  { value: "pending", label: "Pending / WIP" },
  { value: "wip", label: "WIP" },
  { value: "graveyard", label: "Graveyard" },
  { value: "favourites", label: "我的收藏" },
  { value: "mine", label: "我的谱面" },
] as const;

export const sortOptions = [
  { value: "relevance_desc", label: "相关度" },
  { value: "ranked_desc", label: "Rank 日期：新 → 旧" },
  { value: "ranked_asc", label: "Rank 日期：旧 → 新" },
  { value: "difficulty_asc", label: "难度：低 → 高" },
  { value: "difficulty_desc", label: "难度：高 → 低" },
  { value: "plays_desc", label: "游玩次数：高 → 低" },
  { value: "plays_asc", label: "游玩次数：低 → 高" },
  { value: "favourites_desc", label: "收藏数：高 → 低" },
  { value: "favourites_asc", label: "收藏数：低 → 高" },
  { value: "rating_desc", label: "评分：高 → 低" },
  { value: "rating_asc", label: "评分：低 → 高" },
  { value: "title_asc", label: "标题：A → Z" },
  { value: "title_desc", label: "标题：Z → A" },
  { value: "artist_asc", label: "艺术家：A → Z" },
  { value: "artist_desc", label: "艺术家：Z → A" },
] as const;

export const genreOptions = [
  { value: null, label: "全部流派" },
  { value: 1, label: "未指定" },
  { value: 2, label: "Video Game" },
  { value: 3, label: "Anime" },
  { value: 4, label: "Rock" },
  { value: 5, label: "Pop" },
  { value: 6, label: "Other" },
  { value: 7, label: "Novelty" },
  { value: 9, label: "Hip Hop" },
  { value: 10, label: "Electronic" },
  { value: 11, label: "Metal" },
  { value: 12, label: "Classical" },
  { value: 13, label: "Folk" },
  { value: 14, label: "Jazz" },
] as const;

export const languageOptions = [
  { value: null, label: "全部语言" },
  { value: 1, label: "未指定" },
  { value: 2, label: "English" },
  { value: 3, label: "Japanese" },
  { value: 4, label: "Chinese" },
  { value: 5, label: "Instrumental" },
  { value: 6, label: "Korean" },
  { value: 7, label: "French" },
  { value: 8, label: "German" },
  { value: 9, label: "Swedish" },
  { value: 10, label: "Spanish" },
  { value: 11, label: "Italian" },
  { value: 12, label: "Russian" },
  { value: 13, label: "Polish" },
  { value: 14, label: "Other" },
] as const;

/** 创建与后端查询契约一致的初始筛选条件，重置筛选时也以此为基准。 */
export function createDefaultSearchQuery(
  ruleset: Ruleset,
): OnlineBeatmapSearchQuery {
  return {
    query: "",
    ruleset,
    status: "ranked",
    genre: null,
    language: null,
    extras: [],
    include_nsfw: false,
    // Browse default Ranked results in the same order as the official site:
    // newest rank date first. Entering a free-text search switches to relevance.
    sort: "ranked_desc",
    artist: "",
    title: "",
    title_unicode: "",
    source: "",
    mapper: "",
    difficulty: "",
    tags: "",
    ranked_from: "",
    ranked_to: "",
    submitted_from: "",
    submitted_to: "",
    updated_from: "",
    updated_to: "",
    favourites_min: null,
    favourites_max: null,
    stars_min: null,
    stars_max: null,
    bpm_min: null,
    bpm_max: null,
    length_min: null,
    length_max: null,
    ar_min: null,
    ar_max: null,
    cs_min: null,
    cs_max: null,
    od_min: null,
    od_max: null,
    hp_min: null,
    hp_max: null,
    keys_min: null,
    keys_max: null,
    cursor_string: null,
    content_filter: "",
    grade: "",
    played: "",
  };
}

/** 将输入框文本转换为可选数值；空值和无效值统一表示为未筛选。 */
export function parseOptionalNumber(value: string): number | null {
  if (!value.trim()) return null;
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
}

/** 统计偏离默认值的筛选项数量，供筛选按钮显示状态提示。 */
export function activeFilterCount(query: OnlineBeatmapSearchQuery): number {
  const defaults = createDefaultSearchQuery(query.ruleset ?? "osu");
  const ignored = new Set(["query", "ruleset", "cursor_string", "sort"]);
  return Object.entries(query).filter(([key, value]) => {
    if (ignored.has(key)) return false;
    const defaultValue = defaults[key as keyof OnlineBeatmapSearchQuery];
    if (Array.isArray(value)) return value.length > 0;
    return value !== defaultValue;
  }).length;
}

/** 补全 osu! API 可能返回的协议相对或站内相对预览地址。 */
export function normalizePreviewUrl(value?: string): string | null {
  if (!value) return null;
  if (value.startsWith("//")) return `https:${value}`;
  if (value.startsWith("/")) return `https://osu.ppy.sh${value}`;
  return value;
}

/** 从谱面集的各难度计算并格式化星级范围。 */
export function starRange(beatmaps?: OnlineBeatmap[]): string {
  const ratings = (beatmaps ?? [])
    .map((beatmap) => beatmap.difficulty_rating)
    .filter(Number.isFinite)
    .sort((left, right) => left - right);
  if (!ratings.length) return "—";
  const minimum = ratings[0].toFixed(2);
  const maximum = ratings[ratings.length - 1].toFixed(2);
  return minimum === maximum ? `${minimum}★` : `${minimum}–${maximum}★`;
}

export function durationLabel(seconds?: number): string {
  if (seconds === undefined || !Number.isFinite(seconds)) return "—";
  const minutes = Math.floor(seconds / 60);
  return `${minutes}:${Math.round(seconds % 60)
    .toString()
    .padStart(2, "0")}`;
}
