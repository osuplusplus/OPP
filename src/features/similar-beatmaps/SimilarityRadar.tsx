import {
  Legend,
  PolarAngleAxis,
  PolarGrid,
  PolarRadiusAxis,
  Radar,
  RadarChart,
  ResponsiveContainer,
} from "recharts";
import type { DifficultyFeatureVector, ManiaDifficultyVector } from "../../shared/types/osu";
import { maniaSkillProfile } from "./maniaDifficulty";

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

export function SimilarityRadar({
  target,
  comparison,
  compact = false,
}: {
  target: DifficultyFeatureVector | ManiaDifficultyVector;
  comparison?: DifficultyFeatureVector | ManiaDifficultyVector | null;
  compact?: boolean;
}) {
  const mania = "hand_stream" in target;
  const targetProfile = mania ? maniaSkillProfile(target) : null;
  const comparisonProfile = comparison && "hand_stream" in comparison ? maniaSkillProfile(comparison) : null;
  const data = mania
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
    <div className={compact ? "h-40 sm:h-44" : "h-72"}>
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
            tickCount={3}
          />
          <Radar
            dataKey="target"
            fill="var(--theme-primary)"
            fillOpacity={0.18}
            name="参考谱面"
            stroke="var(--theme-primary)"
            strokeWidth={2}
          />
          {comparison ? (
            <Radar
              dataKey="comparison"
              fill="#f472b6"
              fillOpacity={0.12}
              name="候选谱面"
              stroke="#f472b6"
              strokeWidth={2}
            />
          ) : null}
          {comparison ? (
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
