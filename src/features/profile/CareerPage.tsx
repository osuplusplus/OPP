import { useMemo, useState, type ReactNode } from "react";
import {
  Activity,
  Award,
  CalendarDays,
  ChevronLeft,
  ChevronRight,
  ExternalLink,
  FileImage,
  GitCommit,
  Minus,
  Plus,
  RefreshCw,
  Trash2,
  WifiOff,
} from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import {
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import type { DotItemDotProps } from "recharts";
import { useMode } from "../../app/ModeContext";
import { Avatar } from "../../shared/components/Avatar";
import { Badge, Button, Card, EmptyState, SectionTitle, Skeleton } from "../../shared/components/ui";
import { PageHeader } from "../../shared/components/PageHeader";
import { desktopApi } from "../../shared/lib/tauri";
import { APP_TIME_ZONE, dateTime, fullNumber, percent, rulesetLabels } from "../../shared/lib/format";
import type { CareerCalendarDay, CareerDayDetail, CareerStats, Score } from "../../shared/types/osu";
import { careerCalendarKey, useCareerCalendar, useCareerDay, useCareerStatus, useOwnProfile } from "./api";
import { interpolateStats, monthDays, monthRange, weekday } from "./careerModel";

const statLabels: Array<[keyof CareerStats, string, (value: number | null) => string]> = [
  ["pp", "PP", (value) => value == null ? "—" : value.toFixed(2)],
  ["global_rank", "全球排名", (value) => value == null ? "—" : `#${fullNumber(Math.round(value))}`],
  ["country_rank", "地区排名", (value) => value == null ? "—" : `#${fullNumber(Math.round(value))}`],
  ["ranked_score", "排名分", fullNumber],
  ["total_score", "总分", fullNumber],
  ["hit_accuracy", "准确率", percent],
  ["play_count", "游戏次数", fullNumber],
  ["play_time", "游戏时长", (value) => value == null ? "—" : `${(value / 3600).toFixed(1)} 小时`],
  ["total_hits", "总命中数", fullNumber],
  ["maximum_combo", "最大连击", (value) => value == null ? "—" : `${fullNumber(value)}x`],
];

type ChartMetric = "pp" | "global_rank" | "hit_accuracy" | "play_count";
type QueueItem = { icon: ReactNode; tone: string; title: string; detail: string; action?: ReactNode };
type ChartPoint = { date: string; value: number | null; estimated: boolean };
type ChartConfig = {
  key: ChartMetric;
  label: string;
  color: string;
  format: (value: number | null) => string;
  reverse?: boolean;
};

const chartConfigs: ChartConfig[] = [
  { key: "pp", label: "PP", color: "#ff6aa7", format: (value) => value == null ? "—" : `${value.toFixed(2)}pp` },
  { key: "global_rank", label: "全球排名", color: "#a78bfa", format: (value) => value == null ? "—" : `#${fullNumber(Math.round(value))}`, reverse: true },
  { key: "hit_accuracy", label: "准确率", color: "#5ce1e6", format: (value) => value == null ? "—" : percent(value) },
  { key: "play_count", label: "游戏次数", color: "#34d399", format: (value) => value == null ? "—" : fullNumber(value) },
];

function shiftMonth(year: number, month: number, amount: number) {
  const next = new Date(Date.UTC(year, month + amount, 1));
  return { year: next.getUTCFullYear(), month: next.getUTCMonth() };
}

function scoreTitle(score: Score) {
  return score.beatmapset?.title ?? score.beatmap?.version ?? `成绩 #${score.id ?? "—"}`;
}

function scoreUrl(score: Score) {
  return score.beatmap?.id ? `https://osu.ppy.sh/beatmaps/${score.beatmap.id}` : null;
}

function signed(value: number | null, digits = 0) {
  return value == null || !Number.isFinite(value) ? "—" : `${value > 0 ? "+" : ""}${value.toFixed(digits)}`;
}

function fillMonthDays(year: number, month: number, source: CareerCalendarDay[]) {
  const byDate = new Map(source.map((day) => [day.date, day]));
  return monthDays(year, month).map((date) => byDate.get(date) ?? {
    date,
    status: "missing",
    captured_at: null,
    stats: null,
    error: null,
    has_diff: false,
    added_scores: 0,
    removed_scores: 0,
    changed_scores: 0,
    added_medals: 0,
    added_replays: 0,
    added_screenshots: 0,
  });
}

function buildQueue(detail: CareerDayDetail | undefined): QueueItem[] {
  if (!detail) return [];
  const events: QueueItem[] = [];
  for (const item of detail.score_diffs) {
    if (item.kind !== "added" && item.kind !== "removed") continue;
    const position = item.kind === "removed" ? `#${item.before_position}` : `#${item.after_position}`;
    events.push({
      icon: item.kind === "added" ? <Plus className="size-4" /> : <Minus className="size-4" />,
      tone: item.kind === "added" ? "text-emerald-200" : "text-rose-200",
      title: `BP ${item.kind === "added" ? "加入" : "掉出"} ${position}`,
      detail: `${scoreTitle(item.score)} · ${item.score.pp?.toFixed(2) ?? "—"}pp`,
      action: scoreUrl(item.score) ? <button aria-label="打开官方谱面" className="text-slate-500 hover:text-white" onClick={() => void desktopApi.openExternal(scoreUrl(item.score)!)} type="button"><ExternalLink className="size-4" /></button> : undefined,
    });
  }
  for (const item of detail.medal_events) {
    events.push({ icon: <Award className="size-4" />, tone: "text-amber-200", title: "获得新奖章", detail: `奖章 #${item.name}` });
  }
  for (const item of detail.media_events) {
    const isReplay = item.payload?.kind === "replay";
    events.push({ icon: isReplay ? <GitCommit className="size-4" /> : <FileImage className="size-4" />, tone: "text-violet-200", title: isReplay ? "新增回放" : "新增截图", detail: `${item.name} · ${item.client ?? "本地"}` });
  }
  return events;
}

function latestValue(days: CareerCalendarDay[], key: ChartMetric) {
  return [...days].reverse().find((day) => typeof day.stats?.[key] === "number")?.stats?.[key] ?? null;
}

function ChartTooltip({
  active,
  payload,
  label,
  config,
}: {
  active?: boolean;
  payload?: Array<{ value?: number }>;
  label?: string;
  config: ChartConfig;
}) {
  if (!active || !payload?.length) return null;
  return <div className="rounded-xl border border-white/10 bg-[#0c111d]/95 px-3 py-2 shadow-2xl"><p className="text-[10px] text-slate-500">{label}</p><p className="mt-1 font-mono text-xs font-semibold text-white">{config.format(typeof payload[0]?.value === "number" ? payload[0].value : null)}</p></div>;
}

function TrendChart({ days, config }: { days: CareerCalendarDay[]; config: ChartConfig }) {
  const data: ChartPoint[] = days.map((day) => ({ date: day.date.slice(5), value: typeof day.stats?.[config.key] === "number" ? day.stats[config.key] as number : null, estimated: day.status === "interpolated" }));
  const hasValues = data.filter((point) => point.value != null).length > 1;
  return <div className="rounded-xl border border-white/[.06] bg-white/[.02] p-4"><div className="flex items-start justify-between gap-3"><div><p className="text-xs text-slate-500">{config.label}</p><p className="mt-1 font-mono text-lg font-semibold text-white">{config.format(latestValue(days, config.key))}</p></div><span className="mt-1 size-2 rounded-full" style={{ backgroundColor: config.color }} /></div><div className="mt-3 h-44">{hasValues ? <ResponsiveContainer height="100%" width="100%"><LineChart data={data} margin={{ left: 2, right: 8, top: 8, bottom: 0 }}><CartesianGrid stroke="rgba(255,255,255,.045)" vertical={false} /><XAxis axisLine={false} dataKey="date" minTickGap={28} tick={{ fill: "#596177", fontSize: 10 }} tickLine={false} /><YAxis axisLine={false} domain={["dataMin", "dataMax"]} reversed={config.reverse} tick={{ fill: "#596177", fontSize: 10 }} tickFormatter={(value) => config.key === "global_rank" ? `#${fullNumber(Math.round(value))}` : config.key === "pp" ? `${Math.round(value)}` : config.key === "hit_accuracy" ? `${Number(value).toFixed(1)}%` : fullNumber(Math.round(value))} tickLine={false} width={48} /><Tooltip content={<ChartTooltip config={config} />} cursor={{ stroke: "#ffffff22" }} /><Line activeDot={{ fill: config.color, r: 4, strokeWidth: 0 }} connectNulls={false} dataKey="value" dot={(props: DotItemDotProps) => <circle cx={props.cx} cy={props.cy} fill={props.payload?.estimated ? "#0f172a" : config.color} r={props.payload?.estimated ? 2.5 : 1} stroke={config.color} strokeDasharray={props.payload?.estimated ? "2 2" : undefined} strokeWidth={1} />} isAnimationActive={false} stroke={config.color} strokeWidth={2.5} type="monotone" /></LineChart></ResponsiveContainer> : <div className="grid h-full place-items-center text-xs text-slate-600">还没有足够的快照</div>}</div><p className="mt-1 text-[10px] text-slate-600">空心点为连续数值估算</p></div>;
}

function NotificationQueue({ detail }: { detail: CareerDayDetail | undefined }) {
  const queue = buildQueue(detail);
  return <Card className="p-5"><div className="flex items-start justify-between gap-4"><SectionTitle title={detail?.date ? `${detail.date} 的变更通知` : "变更通知"} description="只记录 Top 200 的新增、掉出以及真实的奖牌和本地媒体事件。" /><Activity className="size-5 text-pink-200" /></div><div className="mt-5">{queue.length ? <div className="space-y-1">{queue.map((item, index) => <div className="relative flex gap-3 py-3" key={`${item.title}-${item.detail}-${index}`}><div className="relative z-10 grid size-8 shrink-0 place-items-center rounded-xl border border-white/[.08] bg-[var(--surface-panel)]"><span className={item.tone}>{item.icon}</span></div>{index < queue.length - 1 ? <span className="absolute bottom-0 left-4 top-11 w-px bg-white/[.08]" /> : null}<div className="min-w-0 flex-1"><div className="flex items-start justify-between gap-3"><p className="text-sm font-semibold text-white">{item.title}</p>{item.action}</div><p className="mt-1 truncate text-xs text-slate-500">{item.detail}</p></div></div>)}</div> : <div className="rounded-2xl border border-dashed border-white/[.1] px-6 py-10 text-center"><Activity className="mx-auto size-7 text-slate-600" /><p className="mt-3 text-sm font-semibold text-slate-300">这一天没有变更通知</p><p className="mt-1 text-xs text-slate-600">统计曲线仍会记录当天的数值，只有真实的成绩进出、奖牌或本地媒体事件才会出现在这里。</p></div>}</div></Card>;
}

export function CareerPage() {
  const { ruleset } = useMode();
  const profileQuery = useOwnProfile(ruleset);
  const statusQuery = useCareerStatus();
  const queryClient = useQueryClient();
  const nowParts = new Intl.DateTimeFormat("en-US", { timeZone: APP_TIME_ZONE, year: "numeric", month: "numeric" }).formatToParts(new Date());
  const [month, setMonth] = useState({ year: Number(nowParts.find((part) => part.type === "year")?.value ?? new Date().getFullYear()), month: Number(nowParts.find((part) => part.type === "month")?.value ?? new Date().getMonth() + 1) - 1 });
  const range = monthRange(month.year, month.month);
  const calendarQuery = useCareerCalendar(ruleset, range.start, range.end);
  const days = useMemo(() => interpolateStats(fillMonthDays(month.year, month.month, calendarQuery.data?.days ?? [])), [calendarQuery.data?.days, month.month, month.year]);
  const [selectedDate, setSelectedDate] = useState<string | null>(null);
  const [capturing, setCapturing] = useState(false);
  const selected = selectedDate && days.some((day) => day.date === selectedDate) ? selectedDate : [...days].reverse().find((day) => day.status === "captured")?.date ?? null;
  const dayQuery = useCareerDay(ruleset, selected);
  const calendarSelected = selected ? days.find((day) => day.date === selected) : undefined;
  const detail = dayQuery.data;
  const displayStats = detail?.stats ?? calendarSelected?.stats ?? null;
  const previous = detail?.previous_stats ?? (calendarSelected ? days.slice(0, days.indexOf(calendarSelected)).reverse().find((day) => day.status === "captured")?.stats ?? null : null);
  const activityDays = [...days].filter((day) => day.status === "captured" && day.has_diff).reverse();
  const capture = async () => {
    setCapturing(true);
    try {
      await desktopApi.captureCareerSnapshot(ruleset, true);
      await Promise.all([statusQuery.refetch(), calendarQuery.refetch(), dayQuery.refetch()]);
      await queryClient.invalidateQueries({ queryKey: careerCalendarKey(ruleset, range.start, range.end) });
    } finally {
      setCapturing(false);
    }
  };
  const clearHistory = async () => {
    if (!window.confirm("确定清空全部生涯历史吗？此操作不可恢复。")) return;
    await desktopApi.clearCareerHistory();
    await queryClient.invalidateQueries({ queryKey: ["career-calendar", ruleset] });
    await statusQuery.refetch();
  };
  if (profileQuery.isLoading || statusQuery.isLoading) return <div className="space-y-5"><Skeleton className="h-24" /><Skeleton className="h-96" /></div>;
  const profile = profileQuery.data?.data;
  if (!profile) return <EmptyState icon={<Activity className="size-5" />} title="暂时无法读取生涯数据" description="请先登录 osu!，然后重试。" action={<Button onClick={() => void profileQuery.refetch()}>重试</Button>} />;
  const cells = [...Array.from({ length: weekday(range.start) }, () => null), ...days];
  return <div className="space-y-5">
    <PageHeader title="生涯动态" actions={<div className="flex gap-2"><Button onClick={() => void clearHistory()} size="sm" variant="ghost"><Trash2 className="size-4" />清空历史</Button><Button loading={capturing} onClick={() => void capture()} size="sm"><RefreshCw className="size-4" />立即采集</Button></div>} />
    <Card className="relative overflow-hidden p-6"><div className="absolute inset-0 bg-gradient-to-r from-pink-400/10 via-transparent to-cyan-300/10" /><div className="relative flex flex-wrap items-center gap-4"><Avatar profile={profile} className="size-14 rounded-2xl border-2 border-white/10" /><div className="min-w-0 flex-1"><div className="flex flex-wrap items-center gap-2"><h2 className="text-xl font-bold text-white">{profile.username}</h2><Badge tone="cyan">{rulesetLabels[ruleset]}</Badge>{statusQuery.data?.configured ? <Badge tone="success">历史记录已启用</Badge> : <Badge tone="warning">等待本地数据库</Badge>}</div><p className="mt-1 text-xs text-slate-400">趋势分析、日历和变更通知都来自本地保存的每日快照。</p></div><div className="text-right text-xs text-slate-500"><p>{statusQuery.data?.snapshot_count ?? 0} 条快照</p><p className="mt-1">{statusQuery.data?.latest_date ? `最近 ${statusQuery.data.latest_date}` : "尚未采集"}</p></div></div></Card>
    {!statusQuery.data?.configured ? <Card className="flex items-center gap-3 border-amber-300/15 bg-amber-300/[0.06] p-4 text-sm text-amber-100"><WifiOff className="size-4" />请先在“本地数据”中配置数据库，生涯动态才会保存。</Card> : null}
    <div className="grid gap-5 xl:grid-cols-[minmax(0,1.35fr)_minmax(300px,.65fr)] xl:items-start">
      <Card className="p-5"><div className="flex items-start justify-between gap-4"><SectionTitle title="表现趋势" description="当前月份的每日快照；断档日期会用相邻快照估算。" /><Badge tone="cyan">{month.year} / {String(month.month + 1).padStart(2, "0")}</Badge></div><div className="mt-5 grid gap-4 sm:grid-cols-2">{chartConfigs.map((config) => <TrendChart config={config} days={days} key={config.key} />)}</div></Card>
      <Card className="p-5 xl:sticky xl:top-[calc(var(--titlebar-height)+1rem)]"><div className="flex items-center justify-between gap-3"><SectionTitle title={`${month.year} 年 ${month.month + 1} 月`} description="点击日期查看当天摘要和通知。" /><div className="flex gap-1"><Button aria-label="上个月" onClick={() => setMonth((value) => shiftMonth(value.year, value.month, -1))} size="icon" variant="ghost"><ChevronLeft className="size-4" /></Button><Button aria-label="下个月" onClick={() => setMonth((value) => shiftMonth(value.year, value.month, 1))} size="icon" variant="ghost"><ChevronRight className="size-4" /></Button></div></div><div className="mt-5 grid grid-cols-7 gap-1 text-center text-[10px] text-slate-600">{["日", "一", "二", "三", "四", "五", "六"].map((label) => <span key={label}>{label}</span>)}{cells.map((day, index) => day ? <button aria-label={day.date} className={`min-h-16 rounded-xl border p-2 text-left transition ${day.date === selected ? "border-[var(--theme-primary)] bg-[var(--theme-primary)]/10" : "border-white/[.05] bg-white/[.02] hover:border-white/[.15]"} ${day.status === "interpolated" ? "border-dashed" : ""}`} key={day.date} onClick={() => setSelectedDate(day.date)} type="button"><span className="text-[10px] text-slate-500">{Number(day.date.slice(-2))}</span>{day.stats ? <><span className={`mt-2 block truncate font-mono text-[10px] ${day.status === "interpolated" ? "text-slate-400" : "text-cyan-100"}`}>{day.stats.pp?.toFixed(0)}pp</span><span className="mt-1 block text-[9px] text-slate-600">{day.has_diff ? `${day.added_scores + day.removed_scores + day.added_medals + day.added_replays + day.added_screenshots} 条变更` : day.status === "interpolated" ? "估算" : ""}</span></> : day.status === "unavailable" ? <span className="mt-2 block text-[9px] text-amber-200">失败</span> : null}</button> : <span key={`empty-${index}`} />)}</div><div className="mt-4 space-y-2 text-[10px] text-slate-500"><p><span className="mr-2 inline-block size-2 rounded-full bg-[var(--theme-primary)]" />已采集</p><p><span className="mr-2 inline-block size-2 rounded-full border border-dashed border-slate-500" />估算或断档</p>{activityDays.length ? <p className="pt-1 text-slate-400">本月 {activityDays.length} 天有变更通知</p> : null}</div></Card>
    </div>
    {displayStats ? <Card className="p-5"><div className="flex items-start justify-between"><SectionTitle title={selected ?? "当天摘要"} description={detail?.captured_at ? `采集于 ${dateTime(detail.captured_at)}` : "断档期间的连续数值估算"} /><CalendarDays className="size-5 text-cyan-200" /></div><div className="mt-4 grid grid-cols-2 gap-2 sm:grid-cols-5">{statLabels.map(([key, label, format]) => { const current = displayStats[key] ?? null; const before = previous?.[key] ?? null; const delta = current != null && before != null ? current - before : null; return <div className="rounded-xl border border-white/[.06] bg-white/[.025] p-3" key={key}><p className="text-[10px] text-slate-500">{label}</p><p className="mt-1 font-mono text-sm font-semibold text-white">{format(current)}</p><p className={`mt-1 text-[10px] ${delta != null && delta > 0 ? "text-emerald-200" : delta != null && delta < 0 ? "text-rose-200" : "text-slate-600"}`}>{delta == null ? "—" : signed(delta, key === "pp" || key === "hit_accuracy" ? 2 : 0)}</p></div>; })}</div></Card> : null}
    <NotificationQueue detail={detail} />
  </div>;
}
