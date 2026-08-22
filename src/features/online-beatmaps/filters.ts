import type {
  OnlineBeatmap,
  OnlineBeatmapSearchQuery,
  Ruleset,
} from "../../shared/types/osu";

export const statusOptions = [
  { value: "any", label: "全部状态" },
  // 与 osu! 官网谱面列表使用相同文案和排列，查询值仍沿用 OPP 现有契约。
  { value: "leaderboard", label: "拥有排行榜" },
  { value: "ranked", label: "上架 (Ranked)" },
  { value: "qualified", label: "过审 (Qualified)" },
  { value: "loved", label: "社区喜爱 (Loved)" },
  { value: "favourites", label: "收藏" },
  { value: "pending", label: "待定 (Pending)" },
  { value: "wip", label: "制作中 (WIP)" },
  { value: "graveyard", label: "坟场 (Graveyard)" },
  { value: "mine", label: "我做的谱面" },
] as const;

export const sortOptions = [
  { value: "relevance_desc", label: "相关度" },
  { value: "ranked_desc", label: "上架 (Ranked) 时间：新 → 旧" },
  { value: "ranked_asc", label: "上架 (Ranked) 时间：旧 → 新" },
  { value: "difficulty_asc", label: "难度：低 → 高" },
  { value: "difficulty_desc", label: "难度：高 → 低" },
  { value: "plays_desc", label: "游玩次数：高 → 低" },
  { value: "plays_asc", label: "游玩次数：低 → 高" },
  { value: "favourites_desc", label: "收藏量：高 → 低" },
  { value: "favourites_asc", label: "收藏量：低 → 高" },
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
  { value: 2, label: "电子游戏" },
  { value: 3, label: "动漫" },
  { value: 4, label: "摇滚" },
  { value: 5, label: "流行" },
  { value: 6, label: "其他" },
  { value: 7, label: "新奇" },
  { value: 9, label: "嘻哈" },
  { value: 10, label: "电子" },
  { value: 11, label: "金属" },
  { value: 12, label: "古典" },
  { value: 13, label: "民谣" },
  { value: 14, label: "爵士" },
] as const;

export const languageOptions = [
  { value: null, label: "全部语言" },
  // 顺序与官网保持一致；value 继续对应 osu! API 的语言编号。
  { value: 2, label: "英语" },
  { value: 4, label: "汉语" },
  { value: 7, label: "法语" },
  { value: 8, label: "德语" },
  { value: 11, label: "意大利语" },
  { value: 3, label: "日语" },
  { value: 6, label: "韩语" },
  { value: 10, label: "西班牙语" },
  { value: 9, label: "瑞典语" },
  { value: 12, label: "俄语" },
  { value: 13, label: "波兰语" },
  { value: 5, label: "器乐" },
  { value: 1, label: "未指定" },
  { value: 14, label: "其他" },
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
