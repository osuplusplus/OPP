import {
  Legend,
  PolarAngleAxis,
  PolarGrid,
  PolarRadiusAxis,
  Radar,
  RadarChart,
  ResponsiveContainer,
  Tooltip,
} from "recharts";
import type { DifficultyFeatureVector, ManiaDifficultyVector, ManiaPatternView } from "../../shared/types/osu";
import { mmaRadarValues, type MmaAxisValue } from "./mmaRadar";

const dimensions: Array<{
  key: keyof DifficultyFeatureVector;
  label: string;
}> = [
  { key: "aim", label: "Aim" },
  { key: "speed", label: "Speed" },
  { key: "reading", label: "Reading" },
  { key: "slider", label: "Slider" },
  { key: "overlap", label: "Overlap" },
];

export function MmaCoverageTooltip({
  label,
  series,
}: {
  label: string;
  series: Array<{ name: string; value: MmaAxisValue }>;
}) {
  return (
    <div className="rounded-lg border border-white/15 bg-slate-950/95 p-3 text-xs text-slate-200 shadow-xl">
      <p className="mb-2 font-semibold">{label}</p>
      {series.map(({ name, value }) => (
        <div className="mb-2 last:mb-0" key={name}>
          <p>{name} · {value.coverage === null ? "暂无覆盖数据" : `${(value.coverage * 100).toFixed(1)}% · ${value.seconds?.toFixed(1)}s`}</p>
          {value.specifics ? <p className="mt-1 text-slate-400">{value.specifics}</p> : null}
        </div>
      ))}
    </div>
  );
}

export function SimilarityRadar({
  target,
  comparison,
  compact = false,
  patternView,
  patternViewComparison,
}: {
  target: DifficultyFeatureVector | ManiaDifficultyVector;
  comparison?: DifficultyFeatureVector | ManiaDifficultyVector | null;
  compact?: boolean;
  patternView?: ManiaPatternView | null;
  patternViewComparison?: ManiaPatternView | null;
}) {
  const mania = "hand_stream" in target;
  const targetValues = mmaRadarValues(patternView);
  const comparisonValues = mmaRadarValues(patternViewComparison);
  const showsTarget = !mania || Boolean(patternView);
  const showsComparison = mania ? Boolean(patternViewComparison) : Boolean(comparison && "aim" in comparison);
  const data = mania
    ? targetValues.map((value, index) => ({ dimension: value.dimension, target: value.radius, comparison: comparisonValues[index].radius }))
    : dimensions.map(({ key, label }) => ({
        dimension: label,
        target: target[key],
        comparison: comparison && "aim" in comparison ? comparison[key] : 0,
      }));

  if (mania && !patternView && !patternViewComparison) {
    return <div role="status" className="grid h-40 place-items-center text-xs text-slate-400">暂无 MMA 六维覆盖数据</div>;
  }

  return (
    <div
      aria-label={mania ? "MMA 六维覆盖率雷达图" : "五维难度雷达图"}
      className={compact ? "relative h-40 sm:h-44" : "relative h-72"}
      role="img"
    >
      {mania && (!patternView || !patternViewComparison) ? <p className="absolute inset-x-0 top-0 text-center text-[10px] text-slate-400">{patternView ? "候选谱面" : "参考谱面"}暂无 MMA 数据</p> : null}
      <ResponsiveContainer height="100%" width="100%">
        <RadarChart data={data} outerRadius={compact ? "68%" : "72%"}>
          <PolarGrid gridType="polygon" radialLines stroke="rgba(0,0,0,.72)" strokeWidth={1.35} />
          <PolarAngleAxis
            dataKey="dimension"
            tick={{ fill: "#94a3b8", fontSize: 11 }}
          />
          <PolarRadiusAxis
            axisLine={false}
            domain={[0, 1]}
            tick={false}
            ticks={mania ? [0, 0.2, 0.5, Math.sqrt(0.5), 1] : undefined}
            tickCount={3}
          />
          {mania ? (
            <Tooltip
              content={({ active, label }) => {
                if (!active) return null;
                const index = targetValues.findIndex((value) => value.dimension === label);
                if (index < 0) return null;
                return (
                  <MmaCoverageTooltip
                    label={String(label)}
                    series={[
                      ...(patternView ? [{ name: "参考谱面", value: targetValues[index] }] : []),
                      ...(patternViewComparison ? [{ name: "候选谱面", value: comparisonValues[index] }] : []),
                    ]}
                  />
                );
              }}
            />
          ) : null}
          {showsTarget ? <Radar
            dataKey="target"
            fill="var(--theme-primary)"
            fillOpacity={0.18}
            name="参考谱面"
            stroke="var(--theme-primary)"
            strokeWidth={2}
          /> : null}
          {showsComparison ? (
            <Radar
              dataKey="comparison"
              fill="#f472b6"
              fillOpacity={0.12}
              name="候选谱面"
              stroke="#f472b6"
              strokeWidth={2}
            />
          ) : null}
          {showsComparison ? (
            <Legend
              iconSize={8}
              wrapperStyle={{ color: "#94a3b8", fontSize: 11 }}
            />
          ) : null}
        </RadarChart>
      </ResponsiveContainer>
    </div>
  );
}
