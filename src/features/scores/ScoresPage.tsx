import { useMemo, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { Medal, Pin, RefreshCw, Trophy, X, Zap } from "lucide-react";
import { useMode } from "../../app/ModeContext";
import { ErrorPanel } from "../../shared/components/ErrorPanel";
import { PageHeader } from "../../shared/components/PageHeader";
import { Badge, Button, Card, DataLine, EmptyState, Skeleton } from "../../shared/components/ui";
import { SearchAutocomplete } from "../../shared/components/SearchAutocomplete";
import { DifficultyIcon } from "../../shared/components/DifficultyIcon";
import { desktopApi } from "../../shared/lib/tauri";
import { dateTime, fullNumber, rankTone, scoreMods, scoreTotal } from "../../shared/lib/format";
import type { Score, ScoreCategory } from "../../shared/types/osu";
import { useOwnProfile } from "../profile/api";
import { Link, useSearchParams } from "react-router-dom";
import { useScores } from "./api";

function accuracy(score: Score) { return `${(score.accuracy * 100).toFixed(2)}%`; }
function scoreDate(score: Score) { return score.ended_at ?? score.created_at ?? null; }

function ScoreSkeleton() {
  return <div className="space-y-3">{Array.from({ length: 6 }, (_, index) => <Skeleton className="h-[112px]" key={index} />)}</div>;
}

function positionTone(position: number) {
  if (position === 1) return "border-amber-300/35 bg-amber-300/12 text-amber-100";
  if (position === 2) return "border-slate-200/30 bg-slate-200/10 text-slate-100";
  if (position === 3) return "border-orange-300/35 bg-orange-300/12 text-orange-100";
  return "border-white/[0.08] bg-slate-950/55 text-slate-300";
}

function ScoreRow({ score, position, category, onOpen }: { score: Score; position: number; category: ScoreCategory; onOpen: () => void }) {
  const beatmap = score.beatmap;
  const set = score.beatmapset;
  const cover = set?.covers?.list ?? set?.covers?.card;
  const mods = scoreMods(score);
  const isBest = category === "best";
  const isMedal = position <= 3 && isBest;
  const rankingLabel = isBest ? "BP" : category === "pinned" ? "PIN" : null;
  return (
    <button className="group grid w-full grid-cols-[72px_minmax(0,1.55fr)_minmax(265px,.85fr)_128px] items-center gap-4 overflow-hidden rounded-2xl border border-white/[0.09] bg-[#111725]/90 p-3 text-left outline-none transition hover:-translate-y-px hover:border-white/[0.18] hover:bg-[#172036] focus-visible:ring-2 focus-visible:ring-cyan-300/55" onClick={onOpen} type="button">
      <div className={positionTone(position) + " grid min-h-20 place-items-center rounded-xl border font-mono shadow-inner"}>
        <div className="text-center">{rankingLabel ? <span className="block text-[10px] font-bold uppercase tracking-[0.12em] opacity-70">{rankingLabel}</span> : null}<span className={`${rankingLabel ? "mt-1" : ""} flex items-center justify-center gap-1 text-xl font-black tabular-nums`}>{isMedal ? <Medal className="size-4" aria-label={`第 ${position} 名`} /> : null}{rankingLabel ? `#${position}` : position}</span></div>
      </div>
      <div className="flex min-w-0 items-center gap-3.5">
        <div className="relative h-[76px] w-[114px] shrink-0 overflow-hidden rounded-xl border border-white/[0.08] bg-slate-900">
          {cover ? <img alt="" className="size-full object-cover" src={cover} /> : <div className="grid size-full place-items-center text-slate-500"><Trophy className="size-5" /></div>}
          <span className={`absolute bottom-1.5 left-1.5 grid min-w-8 place-items-center rounded-md border border-slate-300/25 bg-slate-900 px-1.5 py-1 font-mono text-xs font-black leading-none shadow-lg ${rankTone(score.rank)}`}>{score.rank}</span>
        </div>
        <div className="min-w-0"><p className="truncate text-[15px] font-semibold text-white">{set?.title_unicode || set?.title || "未知谱面"}</p><p className="mt-1 truncate text-xs text-slate-400">{set?.artist_unicode || set?.artist || "未知艺术家"} <span className="text-slate-500">· [{beatmap?.version ?? "?"}]</span></p><div className="mt-2 flex flex-wrap gap-1.5">{mods.length ? mods.map((mod) => <span className="rounded-md border border-cyan-200/15 bg-cyan-300/10 px-1.5 py-0.5 font-mono text-[10px] font-bold text-cyan-100" key={mod}>{mod}</span>) : <span className="rounded-md border border-slate-400/15 bg-slate-400/10 px-1.5 py-0.5 font-mono text-[10px] font-bold text-slate-300">NM</span>}</div></div>
      </div>
      <div className="grid grid-cols-2 gap-2.5">
        <Metric label="ACC" tone="text-cyan-100" value={accuracy(score)} />
        <Metric label="COMBO" tone="text-violet-100" value={`${fullNumber(score.max_combo)}x`} />
        <Metric label="STARS" tone="text-amber-100" value={<DifficultyIcon mode={beatmap?.mode} stars={beatmap?.difficulty_rating} />} />
        <Metric label="WEIGHT" tone="text-slate-200" value={`${score.weight?.percentage?.toFixed(0) ?? "—"}%`} />
      </div>
      <div className="text-right"><p className="font-mono text-[26px] font-bold leading-none tracking-tight text-pink-200">{score.pp?.toFixed(2) ?? "—"}<span className="ml-1 text-xs font-semibold text-pink-300/80">pp</span></p><p className="mt-3 text-[11px] text-slate-400">{dateTime(scoreDate(score))}</p></div>
    </button>
  );
}

function Metric({ label, value, tone }: { label: string; value: React.ReactNode; tone: string }) {
  return <div className="score-metric rounded-lg border border-white/[0.065] bg-slate-950/35 px-2.5 py-2"><p className="text-[9px] font-bold tracking-[0.12em] text-slate-500">{label}</p><div className={`mt-1 min-h-4 font-mono text-[13px] font-bold ${tone}`}>{value}</div></div>;
}

function ScoreDialog({ score, position, category, onClose }: { score: Score; position: number; category: ScoreCategory; onClose: () => void }) {
  const beatmap = score.beatmap;
  const set = score.beatmapset;
  const cover = set?.covers?.cover ?? set?.covers?.card;
  const dialogRank = category === "best" ? `BP #${position}` : category === "pinned" ? `PIN #${position}` : String(position);
  return <Dialog.Portal><Dialog.Overlay className="fixed inset-0 z-[80] bg-black/70 backdrop-blur-sm" /><Dialog.Content className="fixed left-1/2 top-1/2 z-[90] w-[720px] max-h-[86vh] -translate-x-1/2 -translate-y-1/2 overflow-auto rounded-[24px] border border-white/10 bg-[#0e1421] shadow-[0_35px_120px_rgba(0,0,0,.65)] outline-none"><div className="relative overflow-hidden p-6">{cover ? <img alt="" className="absolute inset-0 size-full object-cover opacity-30" src={cover} /> : null}<div className="absolute inset-0 bg-gradient-to-t from-[#0e1421] via-[#0e1421]/85 to-[#0e1421]/30" /><div className="relative flex items-end justify-between gap-5"><div className="min-w-0"><Badge tone="pink">{dialogRank}</Badge><Dialog.Title className="mt-3 truncate text-2xl font-semibold text-white">{set?.title_unicode || set?.title || "未知谱面"}</Dialog.Title><Dialog.Description className="mt-2 text-sm text-slate-300">{set?.artist_unicode || set?.artist} · [{beatmap?.version}]</Dialog.Description></div><p className="font-mono text-3xl font-semibold text-pink-200">{score.pp?.toFixed(2) ?? "—"}<span className="ml-1 text-xs text-pink-300">pp</span></p></div></div><div className="grid grid-cols-2 gap-5 p-6"><Card className="p-4"><DataLine label="评级" value={score.rank} /><DataLine label="准确率" value={accuracy(score)} /><DataLine label="最大连击" value={`${fullNumber(score.max_combo)}x`} /><DataLine label="总分" value={fullNumber(scoreTotal(score))} /><DataLine label="Mods" value={scoreMods(score).join(" · ") || "No Mod"} /></Card><Card className="p-4"><DataLine label="星数" value={<DifficultyIcon mode={beatmap?.mode} stars={beatmap?.difficulty_rating} />} /><DataLine label="BPM" value={beatmap?.bpm?.toFixed(0) ?? "—"} /><DataLine label="AR" value={beatmap?.ar?.toFixed(1) ?? "—"} /><DataLine label="OD" value={beatmap?.accuracy?.toFixed(1) ?? "—"} /><DataLine label="获得时间" value={dateTime(scoreDate(score))} /></Card></div><div className="flex justify-end gap-2 border-t border-white/[0.06] px-6 py-4">{beatmap?.url ? <Button onClick={() => void desktopApi.openExternal(beatmap.url!)} size="sm">打开谱面</Button> : null}{score.id ? <Button onClick={() => void desktopApi.openExternal(`https://osu.ppy.sh/scores/${score.id}`)} size="sm" variant="primary">查看成绩</Button> : null}</div><Dialog.Close aria-label="关闭" className="absolute right-4 top-4 grid size-9 shrink-0 place-items-center rounded-xl border border-white/10 bg-black/30 text-slate-300 backdrop-blur-md hover:bg-white/10 hover:text-white" onClick={onClose}><X className="size-4" /></Dialog.Close></Dialog.Content></Dialog.Portal>;
}

export function ScoresPage({ category = "best", offset = 0, title, embedded = false }: { category?: ScoreCategory; offset?: number; title?: string; embedded?: boolean }) {
  const { ruleset } = useMode();
  const [params] = useSearchParams();
  const effectiveOffset = category === "best" ? Number(params.get("offset") ?? offset) || 0 : offset;
  const profileQuery = useOwnProfile(ruleset);
  const scoresQuery = useScores(ruleset, category, effectiveOffset, 100, Boolean(profileQuery.data?.data));
  const [search, setSearch] = useState("");
  const [selected, setSelected] = useState<{ score: Score; position: number } | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const heading = title ?? (category === "pinned" ? "Pinned 成绩" : category === "recent" ? "近期成绩" : effectiveOffset ? "BP 101–200" : "最佳成绩");
  const filtered = useMemo(() => {
    const query = search.trim().toLocaleLowerCase();
    return (scoresQuery.data?.data ?? []).map((score, index) => ({ score, position: index + effectiveOffset + 1 })).filter(({ score }) => !query || [score.beatmapset?.title, score.beatmapset?.title_unicode, score.beatmapset?.artist, score.beatmapset?.creator, score.beatmap?.version, ...scoreMods(score)].filter(Boolean).some((value) => String(value).toLocaleLowerCase().includes(query)));
  }, [scoresQuery.data, search, effectiveOffset]);
  const suggestions = (scoresQuery.data?.data ?? []).flatMap((score) => [score.beatmapset?.title, score.beatmapset?.artist, score.beatmap?.version, ...scoreMods(score)]).filter((value): value is string => Boolean(value)).map((value) => ({ value }));
  const refresh = async () => { setRefreshing(true); try { await scoresQuery.refresh(); } finally { setRefreshing(false); } };
  return <>
    {!embedded ? <><PageHeader title={heading} />{category === "best" ? <div className="mb-4 flex gap-2"><Link className={`rounded-lg px-3 py-2 text-xs ${effectiveOffset === 0 ? "bg-[var(--theme-primary-muted)] text-[var(--theme-primary)]" : "text-slate-500 hover:text-white"}`} to="/data/scores">BP 1–100</Link><Link className={`rounded-lg px-3 py-2 text-xs ${effectiveOffset === 100 ? "bg-[var(--theme-primary-muted)] text-[var(--theme-primary)]" : "text-slate-500 hover:text-white"}`} to="/data/scores?offset=100">BP 101–200</Link></div> : null}</> : <div className="mb-4 flex items-center justify-between"><div className="flex items-center gap-2"><h2 className="text-xl font-semibold tracking-tight text-white">{heading}</h2>{category === "pinned" ? <Pin className="size-4 text-pink-200" /> : null}</div><Badge tone={category === "pinned" ? "pink" : "cyan"}>{category === "pinned" ? "官方置顶" : category === "recent" ? "最近游玩" : `BP ${effectiveOffset + 1}–${effectiveOffset + 100}`}</Badge></div>}
    <Card className="mb-4 flex items-center gap-3 p-3"><div className="flex-1"><SearchAutocomplete aria-label={`搜索${heading}`} className="w-full rounded-xl border border-white/[0.07] bg-black/20 py-2.5 pl-10 pr-4 text-sm text-white outline-none placeholder:text-slate-600 focus:border-cyan-300/25" onChange={setSearch} placeholder="搜索曲名、艺术家、Mapper、难度或 Mod" suggestions={suggestions} value={search} /></div><span className="px-2 text-xs text-slate-400">{filtered.length} / {scoresQuery.data?.data.length ?? 0}</span><Button loading={refreshing} onClick={refresh} size="icon" title="刷新成绩"><RefreshCw className="size-4" /></Button></Card>
    {scoresQuery.isLoading || profileQuery.isLoading ? <ScoreSkeleton /> : scoresQuery.error ? <ErrorPanel error={scoresQuery.error} onRetry={() => scoresQuery.refetch()} /> : filtered.length ? <div className="space-y-2.5">{filtered.map(({ score, position }) => <ScoreRow key={`${score.id ?? "legacy"}-${position}`} category={category} onOpen={() => setSelected({ score, position })} position={position} score={score} />)}</div> : <EmptyState icon={category === "pinned" ? <Pin className="size-5" /> : <Zap className="size-5" />} title={search ? "没有匹配的成绩" : category === "pinned" ? "尚未置顶成绩" : "当前范围没有最佳成绩"} description={search ? "尝试更短的关键词或 Mod 缩写。" : category === "pinned" ? "在 osu! 个人资料中置顶的成绩会显示在这里。" : "切换模式后可能会找到更多记录。"} />}
    <Dialog.Root onOpenChange={(open) => !open && setSelected(null)} open={Boolean(selected)}>{selected ? <ScoreDialog category={category} onClose={() => setSelected(null)} position={selected.position} score={selected.score} /> : null}</Dialog.Root>
  </>;
}
