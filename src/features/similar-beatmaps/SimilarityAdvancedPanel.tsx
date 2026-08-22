import { RotateCcw } from "lucide-react";

import { Button, Card } from "../../shared/components/ui";
import type { DifficultyFeatureVector, OsuSimilarityQueryRequest, SimilarityPreferences } from "../../shared/types/osu";
import { defaultDifficultyWeights } from "./defaults";

const difficultyControls: Array<{ key: keyof DifficultyFeatureVector; label: string }> = [
  { key: "aim", label: "Aim" },
  { key: "speed", label: "Speed" },
  { key: "reading", label: "Reading" },
  { key: "slider", label: "Slider" },
  { key: "overlap", label: "Overlap" },
];

function WeightControl({ label, value, onChange }: { label: string; value: number; onChange: (value: number) => void }) {
  return (
    <label className="block rounded-lg border border-white/[0.07] bg-black/10 px-3 py-2.5">
      <span className="flex items-center justify-between text-xs">
        <span className="text-slate-300">{label}</span>
        <span className="font-mono text-[var(--theme-primary-light)]">{value.toFixed(2)}</span>
      </span>
      <input aria-label={`${label} 权重`} className="mt-2 w-full accent-[var(--theme-primary)]" max="2" min="0" onChange={(event) => onChange(Number(event.target.value))} step="0.05" type="range" value={value} />
    </label>
  );
}

export function SimilarityAdvancedPanel({
  request,
  onChange,
}: {
  request: OsuSimilarityQueryRequest;
  preferences: SimilarityPreferences;
  onChange: (request: OsuSimilarityQueryRequest) => void;
}) {
  const manualWeighting = request.weighting.mode === "manual" ? request.weighting : null;
  const updateManual = (difficulty_weights: DifficultyFeatureVector, parameter_weight: number) => {
    if ([...Object.values(difficulty_weights), parameter_weight].every((value) => value === 0)) return;
    onChange({ ...request, weighting: { mode: "manual", difficulty_weights, parameter_weight } });
  };

  return (
    <Card className="mt-4 p-5">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h3 className="text-sm font-semibold text-white">相似谱面高级参数</h3>
          <p className="mt-1 text-xs leading-5 text-slate-500">按 Analyzer 分类结果使用固定权重排序；AR、CS、OD 共同占一组。</p>
        </div>
      </div>

      {manualWeighting ? (
        <section className="mt-5">
          <div className="mb-2 flex items-center justify-between">
            <p className="text-[10px] font-bold uppercase tracking-[0.18em] text-slate-600">六组推荐权重</p>
            <Button onClick={() => onChange({ ...request, weighting: { mode: "manual", difficulty_weights: { ...defaultDifficultyWeights }, parameter_weight: 1 }, result_limit: 50 })} size="sm" type="button" variant="ghost"><RotateCcw className="size-3.5" />恢复默认</Button>
          </div>
          <div className="grid gap-2 sm:grid-cols-2">
            {difficultyControls.map(({ key, label }) => (
              <WeightControl key={key} label={label} value={manualWeighting.difficulty_weights[key]} onChange={(value) => updateManual({ ...manualWeighting.difficulty_weights, [key]: value }, manualWeighting.parameter_weight)} />
            ))}
            <WeightControl label="AR / CS / OD" value={manualWeighting.parameter_weight} onChange={(parameter_weight) => updateManual(manualWeighting.difficulty_weights, parameter_weight)} />
          </div>
          <p className="mt-3 text-xs text-slate-500">至少保留一组非零权重。</p>
        </section>
      ) : <p className="mt-4 text-xs text-amber-200">旧的动态权重设置将在下次查询时自动迁移为固定权重。</p>}

      <label className="mt-5 block max-w-48 text-xs text-slate-400">
        结果数量
        <select aria-label="结果数量" className="mt-1.5 w-full rounded-lg border border-white/[0.09] bg-[#0b101b] px-3 py-2 text-sm text-slate-100 outline-none focus:border-[var(--theme-primary-soft)]" onChange={(event) => onChange({ ...request, result_limit: Number(event.target.value) })} value={request.result_limit}>
          {[5, 10, 20, 30, 50].map((value) => <option key={value} value={value}>{value} 条</option>)}
        </select>
      </label>
    </Card>
  );
}
