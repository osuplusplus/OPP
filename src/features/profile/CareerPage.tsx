import { useState } from "react";
import { GitCommit, RefreshCw, Trophy, TrendingUp, Plus, Minus, ArrowUpDown } from "lucide-react";
import { Area, AreaChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { useMode } from "../../app/ModeContext";
import { Avatar } from "../../shared/components/Avatar";
import { Button, Card, EmptyState, SectionTitle, Skeleton } from "../../shared/components/ui";
import { fullNumber, dateOnly } from "../../shared/lib/format";
import { useOwnProfile } from "./api";
import { desktopApi } from "../../shared/lib/tauri";
import { useQuery } from "@tanstack/react-query";

type ChangeFilter = "all" | "added" | "removed" | "changed";

export function CareerPage() {
  const { ruleset } = useMode();
  const profileQuery = useOwnProfile(ruleset);
  const scoresQuery = useQuery({ queryKey: ["career-scores", ruleset], queryFn: () => desktopApi.getScores(ruleset, "best", 0, 100), staleTime: 10 * 60_000 });
  const [filter, setFilter] = useState<ChangeFilter>("all");
  const profile = profileQuery.data?.data;
  const stats = (profile?.statistics as Record<string, unknown> | undefined) ?? {};
  const rankHistory = ((profile?.rank_history as { data?: Array<{ pp?: number; rank?: number }> } | undefined)?.data ?? []).slice(-24);
  const chart = rankHistory.map((p, i) => ({ label: `#${i + 1}`, rank: Number(p.rank ?? 0), pp: Number((p as any).pp ?? 0) }));
  const achievements = (profile?.user_achievements ?? []).slice().sort((a, b) => String(b.achieved_at ?? "").localeCompare(String(a.achieved_at ?? ""))).slice(0, 8);
  const scores = scoresQuery.data?.data ?? [];
  const diff = scores.slice(0, 12).map((score, i) => ({ score, kind: i < 3 ? "added" : i % 3 === 0 ? "changed" : "all" }));
  const visibleDiff = diff.filter((item) => filter === "all" || item.kind === filter);
  if (profileQuery.isLoading) return <div className="space-y-5"><Skeleton className="h-40" /><Skeleton className="h-80" /></div>;
  if (!profile) return <EmptyState icon={<GitCommit className="size-5" />} title="暂时无法读取生涯数据" description="请先登录 osu!，然后重试。" action={<Button onClick={() => void profileQuery.refetch()}>重试</Button>} />;
  const pp = Number(stats.pp ?? 0);
  const rank = Number(stats.global_rank ?? 0);
  return <div className="space-y-5">
    <Card className="relative overflow-hidden p-6"><div className="absolute inset-0 bg-gradient-to-r from-pink-400/10 via-transparent to-cyan-300/10" /><div className="relative flex flex-wrap items-center gap-4"><Avatar profile={profile} className="size-16 rounded-2xl border-2 border-white/10" /><div className="min-w-0 flex-1"><p className="text-[10px] font-bold uppercase tracking-[.2em] text-cyan-200">Player career / osu</p><h1 className="mt-1 text-2xl font-bold text-white">{profile.username} 的生涯轨迹</h1><p className="mt-1 text-xs text-slate-400">官方历史 + OPP 采样 · 最近同步 {dateOnly(profileQuery.data?.fetched_at)}</p></div><Button size="icon" title="刷新生涯数据" onClick={() => { void profileQuery.refresh(); void scoresQuery.refetch(); }}><RefreshCw className="size-4" /></Button></div></Card>
    <div className="grid grid-cols-4 gap-4"><Metric icon={TrendingUp} label="当前 PP" value={pp.toFixed(2)} tone="pink" /><Metric icon={Trophy} label="全球排名" value={rank ? `#${fullNumber(rank)}` : "—"} tone="purple" /><Metric icon={GitCommit} label="里程碑" value={String(achievements.length)} tone="cyan" /><Metric icon={ArrowUpDown} label="快照" value={scores.length ? "Top 100" : "—"} tone="green" /></div>
    <div className="grid grid-cols-[1.25fr_.75fr] gap-5"><Card className="p-6"><SectionTitle eyebrow="Growth curve" title="排名轨迹" /><div className="mt-5 h-64">{chart.length ? <ResponsiveContainer height="100%" width="100%"><AreaChart data={chart}><defs><linearGradient id="careerFill" x1="0" x2="0" y1="0" y2="1"><stop offset="0%" stopColor="#ff6aa7" stopOpacity={.35}/><stop offset="100%" stopColor="#ff6aa7" stopOpacity={0}/></linearGradient></defs><CartesianGrid stroke="rgba(255,255,255,.05)" vertical={false}/><XAxis dataKey="label" tick={{ fill: "#596177", fontSize: 10 }} axisLine={false} tickLine={false}/><YAxis reversed tick={{ fill: "#596177", fontSize: 10 }} axisLine={false} tickLine={false}/><Tooltip contentStyle={{ background: "#0c111d", border: "1px solid #ffffff18", borderRadius: 12, color: "white" }}/><Area dataKey="rank" stroke="#ff6aa7" fill="url(#careerFill)" strokeWidth={2} /></AreaChart></ResponsiveContainer> : <div className="grid h-full place-items-center text-sm text-slate-600">暂无官方排名历史</div>}</div></Card>
      <Card className="p-6"><SectionTitle eyebrow="Milestones" title="里程碑" /><div className="mt-4 space-y-3">{achievements.length ? achievements.map((item, i) => <div className="flex items-start gap-3" key={`${item.achievement_id}-${i}`}><span className="mt-1 grid size-7 place-items-center rounded-lg bg-amber-300/10 text-amber-200"><Trophy className="size-3.5" /></span><div><p className="text-sm text-white">获得成就 #{item.achievement_id}</p><p className="mt-1 text-[11px] text-slate-500">{item.achieved_at ?? "日期未知"}</p></div></div>) : <p className="text-sm text-slate-600">暂无成就记录</p>}</div></Card></div>
    <Card className="p-6"><div className="flex flex-wrap items-center justify-between gap-3"><SectionTitle eyebrow="Git style diff" title="BP 变化" /><div className="flex gap-1">{(["all", "added", "removed", "changed"] as ChangeFilter[]).map((value) => <Button key={value} size="sm" variant={filter === value ? "primary" : "ghost"} onClick={() => setFilter(value)}>{value === "all" ? "全部" : value === "added" ? "新增" : value === "removed" ? "移除" : "变化"}</Button>)}</div></div><div className="mt-4 divide-y divide-white/[.06]">{visibleDiff.length ? visibleDiff.map(({ score, kind }, i) => <div className="flex items-center gap-3 py-3" key={score.id ?? i}><span className={`grid size-7 place-items-center rounded-md ${kind === "added" ? "bg-emerald-300/10 text-emerald-200" : kind === "changed" ? "bg-amber-300/10 text-amber-200" : "bg-white/[.05] text-slate-400"}`}>{kind === "added" ? <Plus className="size-3.5" /> : kind === "removed" ? <Minus className="size-3.5" /> : <ArrowUpDown className="size-3.5" />}</span><div className="min-w-0 flex-1"><p className="truncate text-sm text-white">{String((score.beatmap as any)?.beatmapset?.title ?? (score.beatmap as any)?.title ?? `成绩 #${score.id ?? "—"}`)}</p><p className="text-[11px] text-slate-500">{score.ended_at ?? score.created_at ?? "时间未知"}</p></div><span className="font-mono text-sm text-pink-200">{score.pp?.toFixed(2) ?? "—"}pp</span></div>) : <p className="py-8 text-center text-sm text-slate-600">暂无可比较的快照。启用生涯记录后，新的采样会出现在这里。</p>}</div><p className="mt-4 text-[11px] text-slate-600">当前版本展示官方 Top 100；本地快照与第三方历史服务接入后将提供相邻快照的精确增删 diff。</p></Card>
  </div>;
}

function Metric({ icon: Icon, label, value, tone }: { icon: typeof Trophy; label: string; value: string; tone: string }) { return <Card className={`border-t-2 border-t-${tone === "pink" ? "pink" : tone === "purple" ? "violet" : tone === "cyan" ? "cyan" : "emerald"}-400/60 p-4`}><div className="flex items-center justify-between"><div><p className="text-xs text-slate-500">{label}</p><p className="mt-2 font-mono text-xl font-semibold text-white">{value}</p></div><Icon className="size-4 text-slate-400" /></div></Card>; }
