import { useMemo, useState } from "react";
import { CartesianGrid, Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import type { DotItemDotProps } from "recharts";
import type { CareerCalendarDay } from "../../shared/types/osu";
import { buildTrendData, formatCareerStat, type ChartMetric } from "./careerModel";

const metrics: Array<[ChartMetric, string]> = [["pp", "PP"], ["global_rank", "排名"], ["hit_accuracy", "准确率"], ["play_count", "游玩次数"]];

function TrendTooltip({ active, payload, label, metric }: {
  active?: boolean; payload?: Array<{ payload?: { value: number | null; estimate: number | null; estimated: boolean; status: string } }>;
  label?: string | number; metric: ChartMetric;
}) {
  const point = payload?.[0]?.payload;
  if (!active || !point) return null;
  return <div className="career-chart-tooltip"><p>{label}</p><strong>{formatCareerStat(metric, point.value ?? point.estimate)}</strong><p>{point.estimated ? "估算 · 不计入实际成长" : point.status === "partial" ? "真实记录 · 部分采集" : "真实记录 · 点击查看当天"}</p></div>;
}

export function CareerTrend({ days, selected, onSelect }: { days: CareerCalendarDay[]; selected: string | null; onSelect: (date: string) => void }) {
  const [metric, setMetric] = useState<ChartMetric>("pp");
  const [estimates, setEstimates] = useState(false);
  const data = useMemo(() => buildTrendData(days, metric, estimates), [days, metric, estimates]);
  const hasValues = data.some((point) => point.value != null);
  const dot = (props: Pick<DotItemDotProps, "cx" | "cy" | "payload">) => {
    const point = props.payload as { date?: string; value?: number | null } | undefined;
    if (point?.value == null) return <g />;
    return <circle key={point.date} cx={props.cx} cy={props.cy} r={point.date === selected ? 5 : 3} fill="var(--theme-primary)" stroke="var(--surface-panel)" strokeWidth={point.date === selected ? 2 : 1} style={{ cursor: "pointer" }} onClick={(event) => {
      event.stopPropagation();
      if (point.date) onSelect(point.date);
    }} />;
  };
  return <section aria-label="成长趋势" className="career-panel career-trend">
    <div className="career-section-header"><div><h2>成长趋势</h2><p className="career-muted">{days[0]?.date} — {days[days.length - 1]?.date}</p></div>
      <div aria-label="趋势指标" className="career-segments">{metrics.map(([key, label]) => <button aria-pressed={metric === key} key={key} onClick={() => setMetric(key)} type="button">{label}</button>)}</div>
    </div>
    <div className="career-chart">
      {hasValues ? <ResponsiveContainer width="100%" height="100%"><LineChart data={data} margin={{ top: 12, right: 12, left: 4, bottom: 0 }} onClick={(state) => {
        const index = Number(state.activeTooltipIndex);
        const point = state.activeTooltipIndex != null ? data[index] : null;
        if (point?.value != null) onSelect(point.date);
      }}>
        <CartesianGrid vertical={false} stroke="var(--line-subtle)" />
        <XAxis dataKey="date" tickFormatter={(value: string) => value.slice(5)} minTickGap={42} axisLine={false} tickLine={false} tick={{ fill: "var(--text-muted)", fontSize: 11 }} />
        <YAxis width={72} domain={["dataMin", "dataMax"]} reversed={metric === "global_rank"} tickFormatter={(value: number) => formatCareerStat(metric, metric === "pp" || metric === "play_count" ? Math.round(value) : value)} axisLine={false} tickLine={false} tick={{ fill: "var(--text-muted)", fontSize: 11 }} />
        <Tooltip content={<TrendTooltip metric={metric} />} cursor={{ stroke: "var(--line-strong)" }} />
        {estimates ? <Line dataKey="estimate" name="估算" stroke="var(--theme-primary)" strokeOpacity={0.6} strokeDasharray="4 4" strokeWidth={2} dot={false} activeDot={false} connectNulls={false} type="linear" isAnimationActive={false} /> : null}
        <Line dataKey="value" name="真实记录" stroke="var(--theme-primary)" strokeWidth={2.5} dot={dot} activeDot={dot} connectNulls={false} type="linear" isAnimationActive={false} />
      </LineChart></ResponsiveContainer> : <div className="career-chart-empty"><strong>还没有可展示的趋势</strong><p>更新记录后，从第一份真实数据开始记录成长。</p></div>}
    </div>
    <div className="career-chart-foot"><span>实心点为真实记录，点击查看当天详情</span><label><input checked={estimates} onChange={(event) => setEstimates(event.target.checked)} type="checkbox" />显示估算{estimates ? <span> · 虚线</span> : null}</label></div>
  </section>;
}
