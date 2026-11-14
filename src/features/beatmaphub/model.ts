import type { CommandError, RomaiPoolSummary, RomaiTier } from "../../shared/types/osu";

export function hubError(error: unknown) {
  const value = error as CommandError | null;
  return `${value?.message ?? String(error)}${value?.request_id ? `（请求 ID：${value.request_id}）` : ""}`;
}
export function parseHubInput(raw: string): { kind: "code"; id: string; bare: boolean } | { kind: "search"; query: string } | { kind: "invalid" } {
  const text = raw.trim();
  const prefixed = /^BPH-/i.test(text);
  const id = text.replace(/^BPH-/i, "").toUpperCase();
  if (/^[23456789ABCDEFGHJKMNPQRSTUVWXYZ]{6}$/.test(id)) return { kind: "code", id, bare: !prefixed };
  return prefixed ? { kind: "invalid" } : { kind: "search", query: text };
}
export async function copyHubCode(code: string) {
  try {
    if (!navigator.clipboard) return false;
    await navigator.clipboard.writeText(code);
    return true;
  } catch { return false; }
}
/** osu!Collector 集合的更新日期展示；解析失败返回空串（不渲染日期节点）。 */
export function collectorDate(value: string | null) {
  if (!value) return "";
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? "" : date.toLocaleDateString();
}
/** 上传者 osu 头像地址；无用户 ID 时返回 null（不渲染头像）。 */
export function collectorAvatar(uploaderId: number | null) {
  return uploaderId && uploaderId > 0 ? `https://a.ppy.sh/${uploaderId}` : null;
}

// —— osu!Collector 搜索排序（与官网 /all 浏览页同源，无模式/SR 过滤参数）——

/** 排序选项：`sortBy` 为搜索端点字段，含各模式谱面数量排序；`mode` 存在时以模式图标展示。
 * 缺省“相关度”与官网 /all 页一致——空关键词时该排序即按最新上传返回。 */
export const collectorSortOptions = [
  { key: "relevance", label: "相关度", sortBy: "_text_match" },
  { key: "favourites", label: "收藏数", sortBy: "favourites" },
  { key: "recent", label: "最新上传", sortBy: "dateUploaded" },
  { key: "osu", label: "osu!", sortBy: "osuCount", mode: "osu" },
  { key: "taiko", label: "osu!taiko", sortBy: "taikoCount", mode: "taiko" },
  { key: "fruits", label: "osu!catch", sortBy: "catchCount", mode: "fruits" },
  { key: "mania", label: "osu!mania", sortBy: "maniaCount", mode: "mania" },
] as const;
export type CollectorSort = (typeof collectorSortOptions)[number]["key"];

/** 把 UI 排序键映射为搜索端点参数；排序字段始终显式下发。 */
export function collectorSearchParams(query: { search: string; sort: CollectorSort }) {
  const option = collectorSortOptions.find((item) => item.key === query.sort) ?? collectorSortOptions[0];
  return { search: query.search.trim() || null, sortBy: option.sortBy, orderBy: "desc" as const };
}

/** 星数分桶配色（对齐 osu!Collector 官网色阶）；8★ 以上随主题明暗切换。 */
const STAR_BIN_COLORS = [
  "#6EFF79", "#4FC0FF", "#F8DA5E", "#FF7F68", "#FF4E6F", "#A653B0", "#3B38B2",
  "var(--hub-star-high)", "var(--hub-star-high)", "var(--hub-star-high)",
];
export function starBinColor(bin: number) {
  return STAR_BIN_COLORS[Math.min(Math.max(Math.floor(bin), 1), 10) - 1];
}
export interface CommentDraft { content: string; editing: string | null }

// —— RomAI 比赛图池（门槛与配色对齐 romai.dev 图池页）——

/** 段位展示表：顺序即 ELO 升序；`color` 与 RomAI 官网段位徽章同色。 */
export const ROMAI_TIERS: { id: RomaiTier; label: string; color: string; floor: number }[] = [
  { id: "candidate", label: "Candidate", color: "#fb923c", floor: 0 },
  { id: "silver", label: "Silver", color: "#cbd5e1", floor: 1400 },
  { id: "gold", label: "Gold", color: "#fcd34d", floor: 1500 },
  { id: "platinum", label: "Platinum", color: "#5eead4", floor: 1600 },
  { id: "diamond", label: "Diamond", color: "#60a5fa", floor: 1700 },
  { id: "atomos", label: "Atomos", color: "#e2e8f0", floor: 1800 },
  { id: "cosmic", label: "Cosmic", color: "#a78bfa", floor: 1900 },
  { id: "quantum", label: "Quantum", color: "#fb7185", floor: 2000 },
];

export function romaiTierMeta(tier: RomaiTier) {
  return ROMAI_TIERS.find((item) => item.id === tier) ?? ROMAI_TIERS[0];
}

/** 模组配色：与难度分布直方图同源的领域色（NM 蓝/HD 黄/HR 红/DT 紫/FM 绿/TB 亮蓝）。 */
export const ROMAI_MOD_COLORS: Record<string, string> = {
  NM: "#4FC0FF", HD: "#F8DA5E", HR: "#FF7F68", DT: "#A653B0", FM: "#6EFF79", TB: "#4F9DFF",
};

/** 图位展示标签：TB 不带序号，其余为 `NM1`/`HD2` 形式。 */
export function romaiSlotLabel(selectionType: string, position: number) {
  return selectionType === "TB" ? "TB" : `${selectionType}${position}`;
}

/** 卡片模组计数条目：零数量的模组不展示，TB 仅在存在时展示且不带数字。 */
export function romaiModChips(counts: RomaiPoolSummary["counts"]) {
  const entries: { label: string; count: number | null }[] = [
    { label: "NM", count: counts.nm },
    { label: "HD", count: counts.hd },
    { label: "HR", count: counts.hr },
    { label: "DT", count: counts.dt },
    { label: "FM", count: counts.fm },
  ].filter((entry) => entry.count > 0);
  if (counts.tb > 0) entries.push({ label: "TB", count: null });
  return entries;
}

export type RomaiSort = "elo-desc" | "elo-asc" | "name";
export const romaiSortOptions: { key: RomaiSort; label: string }[] = [
  { key: "elo-desc", label: "ELO 高→低" },
  { key: "elo-asc", label: "ELO 低→高" },
  { key: "name", label: "名称" },
];

/** 图池索引的纯筛选/排序：关键词匹配名称，段位过滤后按所选顺序输出。 */
export function romaiFilterPools(pools: RomaiPoolSummary[], query: { search: string; tier: RomaiTier | null; sort: RomaiSort }) {
  const keyword = query.search.trim().toLowerCase();
  const filtered = pools.filter((pool) =>
    (!query.tier || pool.tier === query.tier) && (!keyword || pool.name.toLowerCase().includes(keyword)));
  const sorted = [...filtered];
  if (query.sort === "elo-desc") sorted.sort((a, b) => b.elo - a.elo || a.name.localeCompare(b.name));
  else if (query.sort === "elo-asc") sorted.sort((a, b) => a.elo - b.elo || a.name.localeCompare(b.name));
  else sorted.sort((a, b) => a.name.localeCompare(b.name));
  return sorted;
}

/** 各段位图池计数（段位筛选条目右侧的小数字）。 */
export function romaiTierCounts(pools: RomaiPoolSummary[]) {
  const counts = new Map<RomaiTier, number>();
  for (const pool of pools) counts.set(pool.tier, (counts.get(pool.tier) ?? 0) + 1);
  return counts;
}

/** 单个谱面集封面地址（拼图卡片/弹窗横幅复用）。 */
export function beatmapCoverUrl(setId: number) {
  return `https://assets.ppy.sh/beatmaps/${setId}/covers/cover.jpg`;
}
