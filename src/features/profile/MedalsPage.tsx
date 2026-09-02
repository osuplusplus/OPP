import { useMemo, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { ArrowUpRight, Lock, Medal, Search, X } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Badge, Button, EmptyState, Skeleton } from "../../shared/components/ui";
import { desktopApi } from "../../shared/lib/tauri";
import type { OsekaiMedal, OsekaiMedalBeatmap, Ruleset } from "../../shared/types/osu";
import { useMode } from "../../app/ModeContext";
import { useOwnProfile } from "./api";
import { localizeMedal } from "./medalTranslations";

const base = "https://inex.osekai.net/assets/medals/web/";
function content<T>(value: { content?: unknown[] }): T[] {
  return (value.content ?? []).map((item) => {
    if (!item || typeof item !== "object") return item as T;
    return Object.fromEntries(Object.entries(item).map(([key, value]) => [key, typeof value === "string" ? repairText(value) : value])) as T;
  });
}
function repairText(value: unknown): string {
  if (typeof value !== "string" || !/[ÃÂâ€™œž]/.test(value)) return typeof value === "string" ? value : "";
  try { return new TextDecoder("utf-8", { fatal: true }).decode(Uint8Array.from(value, (c) => c.charCodeAt(0) & 0xff)); } catch { return value; }
}
function unlockedIds(profile: ReturnType<typeof useOwnProfile>["data"]): Map<number, string> {
  const map = new Map<number, string>();
  for (const item of profile?.data.user_achievements ?? []) { const id = Number(item.achievement_id); if (Number.isFinite(id)) map.set(id, String(item.achieved_at ?? "")); }
  return map;
}

function MedalDialog({ medal, obtainedAt, onClose, onOpenBeatmap }: { medal: OsekaiMedal; obtainedAt?: string; onClose: () => void; onOpenBeatmap: (beatmap: OsekaiMedalBeatmap) => void }) {
  const details = useQuery({ queryKey: ["osekai-medal", medal.Medal_ID], queryFn: async () => {
    // The catalogue already contains description/instructions for every
    // medal. Avoid blocking the dialog on the optional, flaky detail endpoint;
    // only request it when the catalogue entry is incomplete.
    const detailPromise = medal.Instructions?.trim() || medal.Solution?.trim()
      ? Promise.resolve(null)
      : desktopApi.getOsekaiMedalDetail(medal.Medal_ID);
    const beatmapsPromise = Promise.race([
      desktopApi.getOsekaiMedalBeatmaps(medal.Medal_ID),
      new Promise<never>((_, reject) => window.setTimeout(() => reject(new Error("timeout")), 2500)),
    ]);
    const [detailResult, beatmapsResult] = await Promise.allSettled([detailPromise, beatmapsPromise]);
    const detail = detailResult.status === "fulfilled" && detailResult.value ? content<OsekaiMedal>(detailResult.value)[0] : undefined;
    const beatmaps = beatmapsResult.status === "fulfilled"
      ? content<OsekaiMedalBeatmap>(beatmapsResult.value).map((item) => ({ ...item, Song_Title: repairText(item.Song_Title), Title: repairText(item.Title), Artist: repairText(item.Artist), Song_Artist: repairText(item.Song_Artist), Version: repairText(item.Version), Difficulty_Name: repairText(item.Difficulty_Name) }))
      : [];
    return { medal: detail ?? medal, beatmaps, detailUnavailable: detailResult.status === "rejected", beatmapsUnavailable: beatmapsResult.status === "rejected" };
  }, staleTime: 24 * 60 * 60_000, retry: 1 });
  const value = localizeMedal(details.data?.medal ?? medal);
  const unlockText = value.Instructions?.trim() || value.Solution?.trim() || "暂无解锁方法";
  return <Dialog.Portal><Dialog.Overlay className="fixed inset-0 z-[80] bg-black/70 backdrop-blur-sm" /><Dialog.Content className="fixed left-1/2 top-1/2 z-[90] max-h-[86vh] w-[min(760px,calc(100vw-2rem))] -translate-x-1/2 -translate-y-1/2 overflow-auto rounded-2xl border border-white/10 bg-[var(--surface-panel)] p-6 shadow-2xl outline-none"><div className="flex items-start gap-5"><div className={`grid size-24 shrink-0 place-items-center rounded-2xl bg-black/25 ${obtainedAt ? "" : "grayscale opacity-60"}`}>{value.Link ? <img alt="" className="max-h-20 max-w-20 object-contain" src={`${base}${value.Link}`} /> : <Medal className="size-9 text-amber-200" />}</div><div className="min-w-0 flex-1"><div className="flex flex-wrap items-center gap-2"><Dialog.Title className="text-xl font-bold text-white">{value.Name ?? `奖牌 #${value.Medal_ID}`}</Dialog.Title><Badge tone={obtainedAt ? "success" : "neutral"}>{obtainedAt ? "已解锁" : "未解锁"}</Badge></div>{obtainedAt ? <p className="mt-1 text-xs text-slate-500">获得于 {obtainedAt}</p> : null}<p className="mt-4 text-sm leading-6 text-slate-300">{value.Description ?? "暂无描述"}</p></div></div>{details.isLoading ? <div className="mt-6 space-y-3"><Skeleton className="h-5 w-1/3" /><Skeleton className="h-20" /></div> : details.error ? <div className="mt-6 rounded-xl border border-amber-300/20 bg-amber-300/5 p-4 text-sm text-amber-100">详情暂不可用。<Button className="ml-3" onClick={() => void details.refetch()} size="sm">重试</Button></div> : <><section className="mt-6 border-t border-white/[0.08] pt-5"><h3 className="text-sm font-semibold text-white">解锁方法</h3><p className="mt-2 whitespace-pre-wrap text-sm leading-6 text-slate-300">{unlockText}</p>{details.data?.detailUnavailable ? <p className="mt-2 text-xs text-amber-200/80">在线详情暂不可用，当前显示奖章目录中的内容。</p> : null}</section><section className="mt-6 border-t border-white/[0.08] pt-5"><h3 className="text-sm font-semibold text-white">Osekai 推荐铺面</h3>{details.data?.beatmaps.length ? <div className="mt-3 space-y-2">{details.data.beatmaps.map((beatmap, index) => { const beatmapId = Number(beatmap.Beatmap_ID); return <button className="flex w-full items-center justify-between rounded-xl border border-white/[0.08] bg-white/[0.025] p-3 text-left text-sm hover:border-cyan-300/30" key={`${beatmap.Beatmap_ID ?? index}`} disabled={!Number.isSafeInteger(beatmapId) || beatmapId <= 0} onClick={() => onOpenBeatmap(beatmap)} type="button"><span className="min-w-0 truncate text-slate-200">{beatmap.Song_Title ?? beatmap.Title ?? `谱面 #${beatmap.Beatmap_ID ?? "—"}`} <span className="text-xs text-slate-500">{beatmap.Difficulty_Name ?? beatmap.Version ? `[${beatmap.Difficulty_Name ?? beatmap.Version}]` : ""}</span></span><ArrowUpRight className="size-3.5 shrink-0 text-cyan-200" /></button>; })}</div> : <p className="mt-2 text-sm text-slate-500">{details.data?.beatmapsUnavailable ? "推荐谱面接口暂不可用" : "Osekai 暂无推荐铺面"}</p>}</section></>}</Dialog.Content><Dialog.Close aria-label="关闭" className="absolute right-4 top-4 text-slate-400 hover:text-white" onClick={onClose}><X className="size-5" /></Dialog.Close></Dialog.Portal>;
}

export function MedalsPage() {
  const navigate = useNavigate();
  const { ruleset } = useMode();
  const profileQuery = useOwnProfile(ruleset as Ruleset);
  const query = useQuery({ queryKey: ["osekai-medals"], queryFn: () => desktopApi.getOsekaiMedals(), staleTime: 24 * 60 * 60_000, retry: 1 });
  const [search, setSearch] = useState(""); const [filter, setFilter] = useState<"all" | "obtained" | "locked">("all"); const [selected, setSelected] = useState<OsekaiMedal | null>(null);
  const obtained = unlockedIds(profileQuery.data);
  const medals = useMemo(() => content<OsekaiMedal>(query.data ?? {}).map(localizeMedal).filter((medal) => { const match = !search.trim() || `${medal.Name ?? ""} ${medal.Description ?? ""}`.toLocaleLowerCase().includes(search.toLocaleLowerCase()); const has = obtained.has(Number(medal.Medal_ID)); return match && (filter === "all" || (filter === "obtained" ? has : !has)); }), [query.data, search, filter, obtained]);
  return <div><div className="flex flex-wrap items-end justify-between gap-4"><div><p className="text-xs font-semibold uppercase tracking-[0.18em] text-[var(--theme-primary)]">osu! medals</p><h2 className="mt-1 text-2xl font-bold text-white">玩家奖牌</h2><p className="mt-1 text-sm text-slate-500">{obtained.size} / {content<OsekaiMedal>(query.data ?? {}).length} 已解锁</p></div><div className="flex gap-2"><div className="relative"><Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-slate-500" /><input aria-label="搜索奖牌" className="opp-input pl-9" onChange={(e) => setSearch(e.target.value)} placeholder="搜索奖牌" value={search} /></div>{(["all", "obtained", "locked"] as const).map((value) => <Button key={value} onClick={() => setFilter(value)} size="sm" variant={filter === value ? "primary" : "ghost"}>{value === "all" ? "全部" : value === "obtained" ? "已解锁" : "未解锁"}</Button>)}</div></div>{query.isLoading ? <div className="mt-5 grid grid-cols-2 gap-3 xl:grid-cols-3">{Array.from({ length: 9 }, (_, i) => <Skeleton className="h-24" key={i} />)}</div> : query.error ? <EmptyState icon={<Medal className="size-5" />} title="奖牌目录暂不可用" description="请检查网络后重试。" action={<Button onClick={() => void query.refetch()}>重试</Button>} /> : medals.length ? <div className="mt-5 grid grid-cols-2 gap-3 xl:grid-cols-3">{medals.map((medal) => { const date = obtained.get(Number(medal.Medal_ID)); return <button className={`flex items-center gap-3 rounded-xl border p-3 text-left transition hover:-translate-y-px hover:border-cyan-300/35 ${date ? "border-amber-300/20 bg-amber-300/[0.045]" : "border-white/[0.07] bg-white/[0.02] grayscale opacity-75 hover:grayscale-0 hover:opacity-100"}`} key={medal.Medal_ID} onClick={() => setSelected(medal)} type="button"><div className="grid size-14 shrink-0 place-items-center">{medal.Link ? <img alt="" className="max-h-12 max-w-12 object-contain" src={`${base}${medal.Link}`} /> : <Medal className="size-6 text-amber-200" />}</div><div className="min-w-0"><p className="truncate text-sm font-semibold text-white">{medal.Name ?? `奖牌 #${medal.Medal_ID}`}</p><p className="mt-1 line-clamp-2 text-xs text-slate-500">{date ? `已解锁 · ${date}` : medal.Description ?? "未解锁"}</p></div>{date ? null : <Lock className="ml-auto size-3.5 shrink-0 text-slate-500" />}</button>; })}</div> : <EmptyState icon={<Search className="size-5" />} title="没有匹配的奖牌" description="尝试其他关键词或筛选条件。" />}<Dialog.Root onOpenChange={(open) => !open && setSelected(null)} open={Boolean(selected)}>{selected ? <MedalDialog medal={selected} onClose={() => setSelected(null)} onOpenBeatmap={(beatmap) => { const params = new URLSearchParams({ beatmap: String(beatmap.Beatmap_ID ?? "") }); const title = beatmap.Song_Title ?? beatmap.Title; if (title?.trim()) params.set("query", title.trim()); setSelected(null); navigate(`/online/beatmaps?${params}`); }} obtainedAt={obtained.get(Number(selected.Medal_ID))} /> : null}</Dialog.Root></div>;
}
