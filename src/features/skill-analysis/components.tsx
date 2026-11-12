import { useEffect, useState } from "react";
import { ArrowUpRight, ExternalLink, Search, WandSparkles, X } from "lucide-react";
import { PolarAngleAxis, PolarGrid, PolarRadiusAxis, Radar, RadarChart, ResponsiveContainer } from "recharts";
import { useNavigate } from "react-router-dom";

import { osuBeatmapUrl } from "../../shared/lib/osuUrl";
import { Badge, Button, Card, Skeleton } from "../../shared/components/ui";
import { desktopApi } from "../../shared/lib/tauri";
import type { OsuClient, SkillAnalysisResult, SkillContribution, SkillVector } from "../../shared/types/osu";
import {
  coveragePercent,
  formatSkill,
  newlyComputed,
  orderedSkills,
  skillDimensions,
  skillRadarData,
  type SkillFocus,
} from "./model";

const MAX_VISIBLE_CONTRIBUTIONS = 60;

// ---------------------------------------------------------------------------
// 加载态
// ---------------------------------------------------------------------------

// analyze_player_skills 是一次性命令，没有逐条进度事件；按耗时分档描述流水线阶段，
// 避免首次分析（需下载谱面）长时间无反馈。
const LOADING_STAGES = [
  { afterSeconds: 0, text: "正在获取最佳成绩…" },
  { afterSeconds: 5, text: "正在解析谱面与 Mods…" },
  { afterSeconds: 12, text: "正在计算八维能力模型…" },
  { afterSeconds: 30, text: "首次分析需要下载谱面数据，耗时较长；结果会缓存 1 小时…" },
];

export function AnalysisLoadingPanel() {
  const [elapsed, setElapsed] = useState(0);
  useEffect(() => {
    const startedAt = Date.now();
    const timer = window.setInterval(() => {
      setElapsed(Math.floor((Date.now() - startedAt) / 1000));
    }, 1000);
    return () => window.clearInterval(timer);
  }, []);
  const stage = [...LOADING_STAGES].reverse().find((item) => elapsed >= item.afterSeconds) ?? LOADING_STAGES[0];
  return (
    <div className="space-y-5">
      <Card className="grid place-items-center p-10 text-center">
        <LoadingRadarGlyph />
        <p aria-live="polite" className="mt-6 text-sm font-medium text-slate-200" role="status">{stage.text}</p>
        <p className="mt-1.5 font-mono text-xs text-slate-500">已进行 {elapsed}s</p>
      </Card>
      <Skeleton className="h-[150px]" />
      <Skeleton className="h-[430px]" />
    </div>
  );
}

// 内联八边形雷达轮廓，animate-pulse 呼吸动效呼应页面主视觉。
function LoadingRadarGlyph() {
  const spokes = Array.from({ length: 8 }, (_, index) => {
    const angle = ((-90 + index * 45) * Math.PI) / 180;
    return { x1: 60, y1: 60, x2: 60 + 46 * Math.cos(angle), y2: 60 + 46 * Math.sin(angle) };
  });
  return (
    <svg aria-hidden="true" className="size-24 animate-pulse" viewBox="0 0 120 120">
      {spokes.map((spoke, index) => (
        <line key={index} stroke="rgba(255,255,255,.14)" strokeWidth={1} x1={spoke.x1} x2={spoke.x2} y1={spoke.y1} y2={spoke.y2} />
      ))}
      <polygon fill="var(--theme-primary)" fillOpacity={0.08} points="60,14 92.5,27.5 106,60 92.5,92.5 60,106 27.5,92.5 14,60 27.5,27.5" stroke="var(--theme-primary)" strokeOpacity={0.55} strokeWidth={1.5} />
      <polygon fill="var(--theme-primary)" fillOpacity={0.14} points="60,32 79.8,40.2 88,60 79.8,79.8 60,88 40.2,79.8 32,60 40.2,40.2" stroke="var(--theme-primary)" strokeOpacity={0.3} strokeWidth={1} />
      <circle cx={60} cy={60} fill="var(--theme-primary)" r={2.5} />
    </svg>
  );
}

// ---------------------------------------------------------------------------
// 玩家与覆盖概览
// ---------------------------------------------------------------------------

export function PlayerOverviewStrip({ result }: { result: SkillAnalysisResult }) {
  const coverage = result.coverage;
  const percent = coveragePercent(coverage);
  return (
    <Card className="p-5">
      <div className="flex items-center gap-4">
        <PlayerAvatar player={result.player} />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <p className="text-base font-semibold text-white">{result.player.username}</p>
            {result.player.country_code ? <Badge tone="cyan">{result.player.country_code}</Badge> : null}
          </div>
          <p className="mt-1 truncate text-xs text-slate-500">
            osuSkills 兼容算法 v{result.algorithm.version} · {new Date(result.fetched_at).toLocaleString()}
          </p>
        </div>
        <div className="shrink-0 text-right">
          <p className="text-[10px] font-bold uppercase tracking-[.2em] text-slate-500">分析覆盖</p>
          <p className="mt-0.5 font-mono text-2xl font-semibold text-white">
            {percent}<span className="text-sm text-slate-500">%</span>
          </p>
          <p className="text-[11px] text-slate-500">{coverage.analyzed_scores} / {coverage.requested_scores} 条成绩</p>
        </div>
      </div>
      <div className="mt-4 h-1.5 overflow-hidden rounded-full bg-white/[.07]">
        <div className="h-full rounded-full bg-[var(--theme-primary)]" style={{ width: `${percent}%` }} />
      </div>
      <div className="mt-3 flex flex-wrap items-center gap-1.5">
        <StatChip label="本地" value={coverage.local_scores} />
        <StatChip label="在线" value={coverage.online_scores} />
        <StatChip label="增量复用" value={coverage.reused_scores ?? 0} />
        <StatChip label="新计算" value={newlyComputed(coverage)} />
        {coverage.skipped_scores ? <StatChip label="跳过" tone="warning" value={coverage.skipped_scores} /> : null}
        {coverage.skipped_reasons.length ? (
          <details className="text-[11px] text-amber-200/80">
            <summary className="cursor-pointer select-none">跳过原因（{coverage.skipped_reasons.length}）</summary>
            <ul className="mt-2 max-h-24 list-disc space-y-1 overflow-auto pl-4">
              {coverage.skipped_reasons.slice(0, 20).map((reason, index) => <li key={`${index}-${reason}`}>{reason}</li>)}
            </ul>
          </details>
        ) : null}
      </div>
    </Card>
  );
}

function PlayerAvatar({ player }: { player: SkillAnalysisResult["player"] }) {
  const source = player.avatar_data_url ?? player.avatar_url;
  const [failed, setFailed] = useState(false);
  if (!source || failed) {
    return (
      <div aria-hidden="true" className="grid size-12 shrink-0 place-items-center rounded-full border border-white/10 bg-white/[.05] text-base font-semibold text-slate-300">
        {player.username.slice(0, 1).toUpperCase()}
      </div>
    );
  }
  return <img alt="" className="size-12 shrink-0 rounded-full border border-white/10 object-cover" onError={() => setFailed(true)} src={source} />;
}

function StatChip({ label, tone = "neutral", value }: { label: string; tone?: "neutral" | "warning"; value: number }) {
  return (
    <span className={`inline-flex items-center gap-1.5 rounded-md border px-2 py-1 text-[11px] ${tone === "warning" ? "border-amber-300/25 bg-amber-300/[.07] text-amber-100" : "border-white/10 bg-white/[.04] text-slate-300"}`}>
      <span className="text-slate-500">{label}</span>
      <span className="font-mono font-semibold">{value}</span>
    </span>
  );
}

// ---------------------------------------------------------------------------
// 能力雷达英雄区
// ---------------------------------------------------------------------------

export function SkillRadarHero({
  focus,
  onSelectFocus,
  skills,
}: {
  focus: SkillFocus;
  onSelectFocus: (focus: SkillFocus) => void;
  skills: SkillVector;
}) {
  const ranked = orderedSkills(skills);
  const highest = ranked[0];
  const lowest = ranked[ranked.length - 1];
  return (
    <Card className="p-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <p className="text-[10px] font-bold uppercase tracking-[.2em] text-[var(--theme-primary)]">Skill profile</p>
          <h2 className="mt-1 text-lg font-semibold text-white">八项能力分布</h2>
        </div>
        <p className="flex flex-wrap items-center gap-3 text-xs text-slate-500">
          <span className="inline-flex items-center gap-1.5">
            <span className="size-1.5 rounded-full bg-[var(--theme-primary)]" />
            最高 {highest?.label ?? "—"} {formatSkill(highest?.value ?? Number.NaN)}
          </span>
          <span className="inline-flex items-center gap-1.5">
            <span className="size-1.5 rounded-full bg-[var(--theme-secondary)]" />
            最低 {lowest?.label ?? "—"} {formatSkill(lowest?.value ?? Number.NaN)}
          </span>
        </p>
      </div>
      <div className="mt-4 grid gap-6 lg:grid-cols-[300px_minmax(0,1fr)]">
        <div>
          <div className="h-[300px]">
            <ResponsiveContainer height="100%" width="100%">
              <RadarChart data={skillRadarData(skills)}>
                <PolarGrid stroke="rgba(255,255,255,.12)" />
                <PolarAngleAxis dataKey="dimension" tick={{ fill: "#94a3b8", fontSize: 11 }} />
                <PolarRadiusAxis axisLine={false} domain={[0, 1000]} tick={false} />
                <Radar dataKey="value" fill="var(--theme-primary)" fillOpacity={0.22} stroke="var(--theme-primary)" strokeWidth={2} />
              </RadarChart>
            </ResponsiveContainer>
          </div>
          <p className="text-center text-[11px] text-slate-600">点击右侧能力条可筛选成绩明细</p>
        </div>
        <div className="space-y-1">
          {ranked.map((item, index) => {
            const active = focus === item.key;
            const isLowest = lowest?.key === item.key;
            return (
              <button
                aria-pressed={active}
                className={`flex w-full items-center gap-3 rounded-lg px-2.5 py-2 text-left transition focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--theme-primary)] ${active ? "bg-[var(--theme-primary-muted)] ring-1 ring-inset ring-[var(--theme-primary-soft)]" : "hover:bg-white/[.04]"}`}
                key={item.key}
                onClick={() => onSelectFocus(active ? "all" : item.key)}
                type="button"
              >
                <span className={`w-4 font-mono text-xs ${index === 0 ? "text-[var(--theme-primary)]" : "text-slate-600"}`}>{index + 1}</span>
                <span className="w-10 shrink-0 text-xs text-slate-300">{item.label}</span>
                <div className="h-2 min-w-0 flex-1 overflow-hidden rounded-full bg-white/[.07]">
                  <div className={`h-full rounded-full ${isLowest ? "bg-[var(--theme-secondary)]" : "bg-[var(--theme-primary)]"}`} style={{ width: `${Math.min(100, item.value / 10)}%` }} />
                </div>
                <span className={`w-10 text-right font-mono text-sm ${isLowest ? "text-[var(--theme-secondary)]" : "text-white"}`}>{formatSkill(item.value)}</span>
              </button>
            );
          })}
        </div>
      </div>
    </Card>
  );
}

// ---------------------------------------------------------------------------
// 短板练习条带
// ---------------------------------------------------------------------------

export function TrainingFocusStrip({
  focus,
  onSelect,
  skills,
}: {
  focus: SkillFocus;
  onSelect: (focus: SkillFocus) => void;
  skills: SkillVector;
}) {
  const weakest = [...orderedSkills(skills)].slice(-4).reverse(); // 最弱在前
  return (
    <Card className="flex flex-wrap items-center gap-3 p-4">
      <div className="flex items-center gap-2.5">
        <div className="grid size-9 shrink-0 place-items-center rounded-lg bg-pink-400/10 text-pink-200">
          <WandSparkles className="size-4" />
        </div>
        <div>
          <p className="text-sm font-semibold text-white">从短板开始练习</p>
          <p className="mt-0.5 text-[11px] text-slate-500">按短板维度筛选成绩明细，优先补齐弱项。</p>
        </div>
      </div>
      <div className="flex flex-1 flex-wrap gap-2">
        {weakest.map((item) => {
          const active = focus === item.key;
          return (
            <button
              aria-pressed={active}
              className={`rounded-lg border px-3 py-1.5 text-left transition ${active ? "border-pink-300/40 bg-pink-300/10 text-pink-100" : "border-white/10 text-slate-300 hover:border-white/20 hover:bg-white/[.05] hover:text-white"}`}
              key={item.key}
              onClick={() => onSelect(active ? "all" : item.key)}
              type="button"
            >
              <span className="block text-xs">{item.label}</span>
              <span className="mt-0.5 block font-mono text-sm">{formatSkill(item.value)}</span>
            </button>
          );
        })}
      </div>
      <div className="flex shrink-0 items-center gap-2">
        {focus !== "all" ? (
          <Button onClick={() => onSelect("all")} size="sm" variant="ghost"><X className="size-4" />清除筛选</Button>
        ) : null}
        <Button onClick={() => onSelect(weakest[0]?.key ?? "all")} size="sm" variant="secondary">
          <WandSparkles className="size-4" />查看最低项贡献
        </Button>
      </div>
    </Card>
  );
}

// ---------------------------------------------------------------------------
// 成绩能力明细
// ---------------------------------------------------------------------------

export function ContributionList({
  client,
  focus,
  items,
  onSearch,
  onSelectFocus,
  search,
}: {
  client: OsuClient;
  focus: SkillFocus;
  items: SkillContribution[];
  onSearch: (search: string) => void;
  onSelectFocus: (focus: SkillFocus) => void;
  search: string;
}) {
  const navigate = useNavigate();
  const shown = items.slice(0, MAX_VISIBLE_CONTRIBUTIONS);
  return (
    <Card className="p-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <p className="text-[10px] font-bold uppercase tracking-[.2em] text-violet-200">Score contributions</p>
          <h2 className="mt-1 text-lg font-semibold text-white">成绩能力明细</h2>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          {focus !== "all" ? (
            <button
              className="inline-flex items-center gap-1 rounded-md border border-pink-300/30 bg-pink-300/10 px-2 py-1.5 text-[11px] text-pink-100 transition hover:bg-pink-300/20"
              onClick={() => onSelectFocus("all")}
              type="button"
            >
              {skillDimensions.find((item) => item.key === focus)?.label}贡献<X className="size-3" />
            </button>
          ) : null}
          <label className="relative">
            <Search className="pointer-events-none absolute left-2.5 top-2.5 size-3.5 text-slate-600" />
            <input className="opp-input h-9 w-48 pl-8 text-xs" onChange={(event) => onSearch(event.target.value)} placeholder="搜索谱面" value={search} />
          </label>
          <select className="opp-input h-9 text-xs" onChange={(event) => onSelectFocus(event.target.value as SkillFocus)} value={focus}>
            <option value="all">按 PP 排序</option>
            {skillDimensions.map((item) => <option key={item.key} value={item.key}>{item.label}贡献</option>)}
          </select>
        </div>
      </div>
      <div className="mt-4 divide-y divide-white/[.06]">
        {shown.map((item, index) => (
          <div className="flex flex-wrap items-center gap-3 py-3" key={`${item.beatmap_id}-${item.source}`}>
            <span className="w-6 shrink-0 text-right font-mono text-xs text-slate-600">{index + 1}</span>
            <div className="min-w-0 flex-1">
              <p className="truncate text-sm font-medium text-white">
                {item.title} <span className="text-slate-500">[{item.version}]</span>
              </p>
              <p className="mt-1 truncate text-[11px] text-slate-500">
                {item.artist} · {item.mods.length ? item.mods.join(" ") : "NM"} · {(item.accuracy * 100).toFixed(2)}%
                {item.combo != null ? ` · ${item.combo}x${item.max_combo ? `/${item.max_combo}x` : ""}` : ""}
                {item.misses ? ` · ${item.misses} miss` : ""} · {item.source === "local" ? "本地" : "在线"}
              </p>
            </div>
            {focus !== "all" ? (
              <span className="w-14 shrink-0 text-right font-mono text-xs text-[var(--theme-primary)]">{formatSkill(item.weighted_skills[focus])}</span>
            ) : null}
            <span className="w-16 shrink-0 text-right font-mono text-sm font-semibold text-pink-100">{item.pp?.toFixed(0) ?? "—"}pp</span>
            <div className="flex shrink-0 items-center gap-1">
              <Button
                aria-label="查找相似谱面"
                onClick={() => navigate(`/online/similar?source=beatmap_id&value=${item.beatmap_id}&ruleset=osu${focus === "all" ? "" : `&focus_skill=${focus}`}`)}
                size="icon"
                title="查找相似谱面"
                variant="ghost"
              >
                <ArrowUpRight className="size-4" />
              </Button>
              {item.resource_id ? (
                <Button
                  aria-label="导入练习生成器"
                  onClick={() => navigate(`/trainer?client=${client}&resource=${globalThis.encodeURIComponent(item.resource_id ?? "")}`)}
                  size="icon"
                  title="导入练习生成器"
                  variant="ghost"
                >
                  <WandSparkles className="size-4" />
                </Button>
              ) : (
                <Button
                  aria-label="打开在线谱面"
                  onClick={() => void desktopApi.openExternal(osuBeatmapUrl(item.beatmap_id))}
                  size="icon"
                  title="打开在线谱面"
                  variant="ghost"
                >
                  <ExternalLink className="size-4" />
                </Button>
              )}
            </div>
          </div>
        ))}
        {!shown.length ? <p className="py-10 text-center text-sm text-slate-600">没有符合筛选条件的成绩。</p> : null}
      </div>
      {items.length > MAX_VISIBLE_CONTRIBUTIONS ? (
        <p className="mt-3 text-center text-xs text-slate-600">仅显示前 {MAX_VISIBLE_CONTRIBUTIONS} 条贡献，完整覆盖率仍按 200 条成绩计算。</p>
      ) : null}
    </Card>
  );
}
