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
  supportsDynamicWeighting,
  preferences,
  onChange,
}: {
  request: OsuSimilarityQueryRequest;
  supportsDynamicWeighting: boolean;
  preferences: SimilarityPreferences;
  onChange: (request: OsuSimilarityQueryRequest) => void;
}) {
  const dynamicWeighting = request.weighting.mode === "dynamic" ? request.weighting : null;
  const manualWeighting = request.weighting.mode === "manual" ? request.weighting : null;
  const updateManual = (difficulty_weights: DifficultyFeatureVector, parameter_weight: number) => {
    if ([...Object.values(difficulty_weights), parameter_weight].every((value) => value === 0)) return;
    onChange({ ...request, weighting: { mode: "manual", difficulty_weights, parameter_weight } });
  };
  const setMode = (mode: "dynamic" | "manual") => {
    onChange({
      ...request,
      weighting: mode === "dynamic"
        ? { mode: "dynamic", lower_sections: preferences.lower_sections, upper_sections: preferences.upper_sections }
        : {
            mode: "manual",
            difficulty_weights: {
              aim: preferences.manual_weights.aim,
              speed: preferences.manual_weights.speed,
              reading: preferences.manual_weights.reading,
              slider: preferences.manual_weights.slider,
              overlap: preferences.manual_weights.overlap,
            },
            parameter_weight: preferences.manual_weights.parameters,
          },
    });
  };

  return (
    <Card className="mt-4 p-5">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h3 className="text-sm font-semibold text-white">相似谱面高级参数</h3>
          <p className="mt-1 text-xs leading-5 text-slate-500">动态模式按同星段突出特征自动分配六组权重；AR、CS、OD 共同占一组。</p>
        </div>
        <div className="inline-flex rounded-lg border border-white/[0.08] bg-black/15 p-1" role="tablist" aria-label="相似度权重模式">
          <Button aria-selected={request.weighting.mode === "dynamic"} disabled={!supportsDynamicWeighting} onClick={() => setMode("dynamic")} role="tab" size="sm" type="button" variant={request.weighting.mode === "dynamic" ? "primary" : "ghost"}>动态</Button>
          <Button aria-selected={request.weighting.mode === "manual"} onClick={() => setMode("manual")} role="tab" size="sm" type="button" variant={request.weighting.mode === "manual" ? "primary" : "ghost"}>手动</Button>
        </div>
      </div>

      {!supportsDynamicWeighting && preferences.mode === "dynamic" ? (
        <p className="mt-4 rounded-lg border border-amber-300/20 bg-amber-300/10 px-3 py-2 text-xs text-amber-100">
          当前索引不含动态统计，已切换为手动权重。
        </p>
      ) : null}

      {dynamicWeighting ? (
        <section className="mt-5">
          <div className="grid gap-3 sm:grid-cols-2">
            {([
              ["lower_sections", "向下范围"],
              ["upper_sections", "向上范围"],
            ] as const).map(([key, label]) => (
              <label className="rounded-lg border border-white/[0.07] bg-black/10 px-3 py-2.5 text-xs text-slate-300" key={key}>
                <span className="flex items-center justify-between"><span>{label}</span><span className="font-mono text-[var(--theme-primary-light)]">{dynamicWeighting[key]} 段</span></span>
                <input aria-label={label} className="mt-2 w-full accent-[var(--theme-primary)]" min="0" max="20" step="1" type="range" value={dynamicWeighting[key]} onChange={(event) => onChange({ ...request, weighting: { ...dynamicWeighting, [key]: Number(event.target.value) } })} />
              </label>
            ))}
          </div>
          <p className="mt-3 text-xs text-slate-500">每段固定为 0.1★，上下范围分别计算，不会改变候选筛选条件。</p>
        </section>
      ) : manualWeighting ? (
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
      ) : null}

      <label className="mt-5 block max-w-48 text-xs text-slate-400">
        结果数量
        <select aria-label="结果数量" className="mt-1.5 w-full rounded-lg border border-white/[0.09] bg-[#0b101b] px-3 py-2 text-sm text-slate-100 outline-none focus:border-[var(--theme-primary-soft)]" onChange={(event) => onChange({ ...request, result_limit: Number(event.target.value) })} value={request.result_limit}>
          {[5, 10, 20, 30, 50].map((value) => <option key={value} value={value}>{value} 条</option>)}
        </select>
      </label>
    </Card>
  );
}
