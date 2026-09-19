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
import { maniaSkillProfile } from "./maniaDifficulty";
import { mmaRadarValues, showsRadarComparison, type MmaAxisValue } from "./mmaRadar";

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

const maniaDimensions: Array<{
  key: keyof ManiaDifficultyVector;
  label: string;
}> = [
  { key: "speed", label: "Speed" },
  { key: "hand_stream", label: "Hand" },
  { key: "jack", label: "Jack" },
  { key: "chordjack", label: "Chordjack" },
  { key: "technical", label: "Technical" },
  { key: "stamina", label: "Stamina" },
  { key: "long_note", label: "LN" },
  { key: "course", label: "Course" },
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
  const targetProfile = mania ? maniaSkillProfile(target) : null;
  const comparisonProfile = comparison && "hand_stream" in comparison ? maniaSkillProfile(comparison) : null;
  const targetValues = mmaRadarValues(patternView);
  const comparisonValues = mmaRadarValues(patternViewComparison);
  const showsComparison = showsRadarComparison(patternView, patternViewComparison, Boolean(comparison));
  const data = patternView
    ? targetValues.map((value, index) => ({ dimension: value.dimension, target: value.radius, comparison: comparisonValues[index].radius }))
    : mania
      ? maniaDimensions.map(({ key, label }) => ({
          dimension: label,
          target: Number(targetProfile?.[key] ?? 0),
          comparison: Number(comparisonProfile?.[key] ?? 0),
        }))
      : dimensions.map(({ key, label }) => ({
          dimension: label,
          target: target[key],
          comparison: comparison && "aim" in comparison ? comparison[key] : 0,
        }));

  return (
    <div
      aria-label={patternView ? "MMA 六维覆盖率雷达图" : mania ? "Mania 八维难度雷达图" : "五维难度雷达图"}
      className={compact ? "h-40 sm:h-44" : "h-72"}
      role="img"
    >
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
            ticks={patternView ? [0, 0.2, 0.5, Math.sqrt(0.5), 1] : undefined}
            tickCount={3}
          />
          {patternView ? (
            <Tooltip
              content={({ active, label }) => {
                if (!active) return null;
                const index = targetValues.findIndex((value) => value.dimension === label);
                if (index < 0) return null;
                return (
                  <MmaCoverageTooltip
                    label={String(label)}
                    series={[
                      { name: "参考谱面", value: targetValues[index] },
                      ...(patternViewComparison ? [{ name: "候选谱面", value: comparisonValues[index] }] : []),
                    ]}
                  />
                );
              }}
            />
          ) : null}
          <Radar
            dataKey="target"
            fill="var(--theme-primary)"
            fillOpacity={0.18}
            name="参考谱面"
            stroke="var(--theme-primary)"
            strokeWidth={2}
          />
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
