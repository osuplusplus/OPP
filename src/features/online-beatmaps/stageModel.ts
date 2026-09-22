import type { OnlineBeatmapSearchQuery, OnlineBeatmapset, Ruleset } from "../../shared/types/osu";
import { createDefaultSearchQuery, genreOptions, languageOptions, statusOptions } from "./filters";
import { APP_TIME_ZONE } from "../../shared/lib/format";

export type OnlineView = "home" | "results" | "stage";
export type StageOrigin = "home" | "results" | "external";
export const TREND_WINDOW_DAYS = 7;

export function trendingQuery(ruleset: Ruleset, now = new Date()): OnlineBeatmapSearchQuery {
  const since = new Date(now.getTime() - TREND_WINDOW_DAYS * 24 * 60 * 60_000);
  const parts = new Intl.DateTimeFormat("en-CA", { timeZone: APP_TIME_ZONE, year: "numeric", month: "2-digit", day: "2-digit" }).formatToParts(since);
  const part = (type: string) => parts.find((value) => value.type === type)?.value;
  return { ...createDefaultSearchQuery(ruleset), ranked_from: `${part("year")}-${part("month")}-${part("day")}`, sort: "favourites_desc", include_nsfw: false };
}

export function uniqueBeatmapsets(items: OnlineBeatmapset[]) {
  return [...new Map(items.map((item) => [item.id, item])).values()];
}
export const displayTitle = (set: OnlineBeatmapset) => set.title_unicode?.trim() || set.title;
export const displayArtist = (set: OnlineBeatmapset) => set.artist_unicode?.trim() || set.artist;
export function onlineResultsTitle(query: OnlineBeatmapSearchQuery) {
  return [query.query, query.title, query.title_unicode, query.artist, query.mapper, query.source, query.tags, query.difficulty].some((value) => value.trim()) ? "搜索结果" : "在线谱面";
}

export function searchSameTitle(query: OnlineBeatmapSearchQuery, set: OnlineBeatmapset): OnlineBeatmapSearchQuery {
  const originalTitle = set.title_unicode?.trim();
  return { ...query, query: "", title: originalTitle ? "" : set.title, title_unicode: originalTitle || "", sort: "relevance_desc", cursor_string: null };
}
export function pickRandomBeatmapset(items: OnlineBeatmapset[], currentId: number | null, random = Math.random) {
  const candidates = items.length > 1 ? items.filter((item) => item.id !== currentId) : items;
  if (!candidates.length) return null;
  return candidates[Math.min(candidates.length - 1, Math.floor(random() * candidates.length))];
}
export function clearOnlineFilters(query: OnlineBeatmapSearchQuery, ruleset: Ruleset): OnlineBeatmapSearchQuery {
  return { ...createDefaultSearchQuery(ruleset), query: query.query, ruleset: null, status: "any", sort: query.sort };
}
const labels: Partial<Record<keyof OnlineBeatmapSearchQuery, string>> = {
  artist: "艺术家", title: "标题", title_unicode: "原歌曲名", mapper: "谱师", source: "来源", tags: "标签", difficulty: "难度名",
  ranked_from: "上架起始", ranked_to: "上架截止", submitted_from: "提交起始", submitted_to: "提交截止", updated_from: "更新起始", updated_to: "更新截止",
  favourites_min: "最低收藏", favourites_max: "最高收藏", stars_min: "最低星级", stars_max: "最高星级", bpm_min: "最低 BPM", bpm_max: "最高 BPM",
  length_min: "最短秒数", length_max: "最长秒数", ar_min: "最低 AR", ar_max: "最高 AR", cs_min: "最低 CS", cs_max: "最高 CS",
  od_min: "最低 OD", od_max: "最高 OD", hp_min: "最低 HP", hp_max: "最高 HP", keys_min: "最少键数", keys_max: "最多键数",
  content_filter: "内容", grade: "成绩", played: "游玩记录",
};
const contentLabels: Record<string, string> = { recommended: "推荐难度", converts: "包括转谱", follows: "已关注谱师", spotlights: "聚光灯谱面", featured_artists: "精选艺术家", played: "玩过", unplayed: "没玩过" };
export function filterChips(query: OnlineBeatmapSearchQuery) {
  const chips: { key: keyof OnlineBeatmapSearchQuery; label: string; clear: Partial<OnlineBeatmapSearchQuery> }[] = [];
  if (query.ruleset) chips.push({ key: "ruleset", label: query.ruleset, clear: { ruleset: null } });
  if (query.status !== "any") chips.push({ key: "status", label: statusOptions.find((option) => option.value === query.status)?.label ?? query.status, clear: { status: "any" } });
  if (query.genre !== null) chips.push({ key: "genre", label: genreOptions.find((option) => option.value === query.genre)?.label ?? "流派", clear: { genre: null } });
  if (query.language !== null) chips.push({ key: "language", label: languageOptions.find((option) => option.value === query.language)?.label ?? "语言", clear: { language: null } });
  if (query.include_nsfw) chips.push({ key: "include_nsfw", label: "显示不良内容", clear: { include_nsfw: false } });
  if (query.extras.length) chips.push({ key: "extras", label: query.extras.map((value) => value === "video" ? "视频" : "故事板").join("、"), clear: { extras: [] } });
  for (const [key, label] of Object.entries(labels)) {
    const field = key as keyof OnlineBeatmapSearchQuery;
    const value = query[field];
    if (value === null || value === "" || value === undefined) continue;
    chips.push({ key: field, label: `${label} · ${contentLabels[String(value)] ?? value}`, clear: { [field]: typeof value === "number" ? null : "" } });
  }
  return chips;
}
