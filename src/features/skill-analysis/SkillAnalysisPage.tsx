import { useMemo, useState } from "react";
import { ArrowUpRight, Brain, ExternalLink, RefreshCw, Search, Sparkles, WandSparkles } from "lucide-react";
import { PolarAngleAxis, PolarGrid, PolarRadiusAxis, Radar, RadarChart, ResponsiveContainer } from "recharts";
import { useNavigate } from "react-router-dom";

import { useMode } from "../../app/ModeContext";
import { ErrorPanel } from "../../shared/components/ErrorPanel";
import { PageHeader } from "../../shared/components/PageHeader";
import { Button, Card, EmptyState, Skeleton } from "../../shared/components/ui";
import { desktopApi } from "../../shared/lib/tauri";
import type { SkillDimension } from "../../shared/types/osu";
import { useSkillAnalysis } from "./api";
import { formatSkill, orderedSkills, skillDimensions, skillRadarData } from "./model";

const encodeURIComponent = (value: string | null) => globalThis.encodeURIComponent(value ?? "");

export function SkillAnalysisPage() {
  const { client } = useMode();
  const navigate = useNavigate();
  const query = useSkillAnalysis(client);
  const [focus, setFocus] = useState<SkillDimension | "all">("all");
  const [search, setSearch] = useState("");
  const result = query.data;
  const visible = useMemo(() => {
    const normalized = search.trim().toLowerCase();
    return (result?.contributions ?? []).filter((item) => {
      const matchesSearch = !normalized || `${item.title} ${item.artist} ${item.version}`.toLowerCase().includes(normalized);
      return matchesSearch && (focus === "all" || item.weighted_skills[focus] > 0);
    }).sort((left, right) => focus === "all" ? (right.pp ?? 0) - (left.pp ?? 0) : right.weighted_skills[focus] - left.weighted_skills[focus]);
  }, [focus, result?.contributions, search]);

  if (query.isLoading) return <div className="space-y-5"><PageHeader eyebrow="Player analysis" title="技能分析" description="正在读取最佳成绩并计算能力模型…" /><Skeleton className="h-64" /><Skeleton className="h-96" /></div>;
  if (query.error && !result) return <><PageHeader eyebrow="Player analysis" title="技能分析" description="读取玩家成绩或谱面时遇到问题。" /><ErrorPanel error={query.error} /><Button onClick={() => void query.refetch()}><RefreshCw className="size-4" />重试</Button></>;
  if (!result) return <><PageHeader eyebrow="Player analysis" title="技能分析" description="分析自己的 osu! standard 能力分布。" /><EmptyState icon={<Brain className="size-6" />} title="还没有技能分析结果" description="请先登录 osu!，然后开始分析最佳成绩。" action={<Button onClick={() => void query.refetch()}><Sparkles className="size-4" />开始分析</Button>} /></>;

  const top = orderedSkills(result.skills);
  const lowest = top[top.length - 1];
  const coverage = result.coverage;
  return <div className="space-y-5">
    <PageHeader eyebrow="Player analysis · osu! standard" title={`${result.player.username} 的技能分析`} description={`OPP osuSkills-compatible v${result.algorithm.version} · ${new Date(result.fetched_at).toLocaleString()}`} actions={<Button loading={query.isFetching || query.isRefreshing} onClick={() => void query.refresh().catch(() => undefined)} size="sm"><RefreshCw className="size-4" />增量刷新</Button>} />
    {result.stale || query.refreshError ? <Card className="border-amber-300/20 bg-amber-300/[.05] p-4"><p className="text-sm font-semibold text-amber-100">当前显示缓存结果</p><p className="mt-1 text-xs text-amber-100/70">{query.refreshError ? "刷新失败，保留上一次分析结果。" : "网络不可用时仍可查看上一次分析结果。"}</p></Card> : null}
    <Card className="flex flex-wrap items-center gap-4 border-cyan-300/15 bg-cyan-300/[0.05] p-4"><div className="grid size-10 place-items-center rounded-xl bg-cyan-300/10 text-cyan-100"><Brain className="size-5" /></div><div className="min-w-0 flex-1"><p className="text-sm font-semibold text-white">分析覆盖 {coverage.analyzed_scores} / {coverage.requested_scores} 条成绩</p><p className="mt-1 text-xs text-slate-400">本地 {coverage.local_scores} · 在线 {coverage.online_scores} · 增量复用 {coverage.reused_scores ?? 0} · 新计算 {Math.max(0, coverage.analyzed_scores - (coverage.reused_scores ?? 0))} · 跳过 {coverage.skipped_scores}。</p>{coverage.skipped_reasons.length ? <details className="mt-2 text-xs text-amber-200/80"><summary className="cursor-pointer">查看跳过原因（{coverage.skipped_reasons.length}）</summary><ul className="mt-2 max-h-24 list-disc space-y-1 overflow-auto pl-4">{coverage.skipped_reasons.slice(0, 20).map((reason, index) => <li key={`${index}-${reason}`}>{reason}</li>)}</ul></details> : null}</div><span className="font-mono text-lg font-semibold text-cyan-100">{coverage.requested_scores ? Math.round(coverage.analyzed_scores / coverage.requested_scores * 100) : 0}%</span></Card>
    <div className="grid gap-5 xl:grid-cols-[minmax(0,1fr)_360px]">
      <Card className="p-5"><div className="flex items-center justify-between gap-3"><div><p className="text-[10px] font-bold uppercase tracking-[.2em] text-[var(--theme-primary)]">Skill profile</p><h2 className="mt-1 text-lg font-semibold text-white">八项能力分布</h2></div><span className="text-xs text-slate-500">最高：{top[0]?.label ?? "—"} · 最低：{lowest?.label ?? "—"}</span></div><div className="mt-3 grid gap-5 lg:grid-cols-[minmax(0,1fr)_260px]"><div className="space-y-3">{top.map((item) => <div className="flex items-center gap-3" key={item.key}><span className="w-12 text-xs text-slate-400">{item.label}</span><div className="h-2 min-w-0 flex-1 overflow-hidden rounded-full bg-white/[.07]"><div className="h-full rounded-full bg-[var(--theme-primary)]" style={{ width: `${Math.min(100, item.value / 10)}%` }} /></div><span className="w-10 text-right font-mono text-sm text-white">{formatSkill(item.value)}</span></div>)}</div><div className="h-64"><ResponsiveContainer height="100%" width="100%"><RadarChart data={skillRadarData(result.skills)}><PolarGrid stroke="rgba(255,255,255,.12)" /><PolarAngleAxis dataKey="dimension" tick={{ fill: "#94a3b8", fontSize: 10 }} /><PolarRadiusAxis domain={[0, 1000]} tick={false} axisLine={false} /><Radar dataKey="value" fill="var(--theme-primary)" fillOpacity={.22} stroke="var(--theme-primary)" strokeWidth={2} /></RadarChart></ResponsiveContainer></div></div></Card>
      <Card className="p-5"><p className="text-[10px] font-bold uppercase tracking-[.2em] text-pink-200">Training focus</p><h2 className="mt-1 text-lg font-semibold text-white">从短板开始练习</h2><p className="mt-2 text-xs leading-5 text-slate-500">选择能力维度后，明细会优先显示对该能力贡献最高的成绩。</p><div className="mt-4 grid grid-cols-2 gap-2">{top.slice(0, 4).map((item) => <button className={`rounded-lg border px-3 py-2 text-left text-xs transition ${focus === item.key ? "border-pink-300/40 bg-pink-300/10 text-pink-100" : "border-white/10 text-slate-400 hover:bg-white/[.05] hover:text-white"}`} key={item.key} onClick={() => setFocus(item.key)} type="button"><span className="block">{item.label}</span><span className="mt-1 block font-mono text-sm">{formatSkill(item.value)}</span></button>)}</div><Button className="mt-4 w-full" onClick={() => setFocus(lowest?.key ?? "all")} variant="secondary"><WandSparkles className="size-4" />查看最低项贡献</Button></Card>
    </div>
    <Card className="p-5"><div className="flex flex-wrap items-center justify-between gap-3"><div><p className="text-[10px] font-bold uppercase tracking-[.2em] text-violet-200">Score contributions</p><h2 className="mt-1 text-lg font-semibold text-white">成绩能力明细</h2></div><div className="flex flex-wrap gap-2"><label className="relative"><Search className="pointer-events-none absolute left-2.5 top-2.5 size-3.5 text-slate-600" /><input className="opp-input h-9 w-48 pl-8 text-xs" onChange={(event) => setSearch(event.target.value)} placeholder="搜索谱面" value={search} /></label><select className="opp-input h-9 text-xs" onChange={(event) => setFocus(event.target.value as SkillDimension | "all")} value={focus}><option value="all">按 PP 排序</option>{skillDimensions.map((item) => <option key={item.key} value={item.key}>{item.label}贡献</option>)}</select></div></div><div className="mt-4 divide-y divide-white/[.06]">{visible.slice(0, 60).map((item) => <div className="flex flex-wrap items-center gap-3 py-3" key={`${item.beatmap_id}-${item.source}`}><div className="min-w-0 flex-1"><p className="truncate text-sm font-medium text-white">{item.title} <span className="text-slate-500">[{item.version}]</span></p><p className="mt-1 truncate text-[11px] text-slate-500">{item.artist} · {item.mods.length ? item.mods.join(" ") : "NM"} · {(item.accuracy * 100).toFixed(2)}% · {item.misses} miss · {item.source === "local" ? "本地" : "在线"}</p></div><div className="flex shrink-0 items-center gap-3"><span className="font-mono text-sm text-pink-100">{item.pp?.toFixed(0) ?? "—"}pp</span><Button aria-label="查找相似谱面" onClick={() => navigate(`/online/similar?source=beatmap_id&value=${item.beatmap_id}&ruleset=osu${focus === "all" ? "" : `&focus_skill=${focus}`}`)} size="icon" title="查找相似谱面" variant="ghost"><ArrowUpRight className="size-4" /></Button>{item.resource_id ? <Button aria-label="导入练习生成器" onClick={() => navigate(`/trainer?client=${client}&resource=${encodeURIComponent(item.resource_id)}`)} size="icon" title="导入练习生成器" variant="ghost"><WandSparkles className="size-4" /></Button> : <Button aria-label="打开在线谱面" onClick={() => void desktopApi.openExternal(`https://osu.ppy.sh/beatmaps/${item.beatmap_id}`)} size="icon" title="打开在线谱面" variant="ghost"><ExternalLink className="size-4" /></Button>}</div></div>)}{!visible.length ? <p className="py-10 text-center text-sm text-slate-600">没有符合筛选条件的成绩。</p> : null}</div>{visible.length > 60 ? <p className="mt-3 text-center text-xs text-slate-600">仅显示前 60 条贡献，完整覆盖率仍按 200 条成绩计算。</p> : null}</Card>
  </div>;
}
