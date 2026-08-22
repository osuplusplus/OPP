import { Button, Card } from "../../shared/components/ui";
import type {
  AnySimilarityBeatmap,
  AnySimilarityResult,
  ManiaDifficultyVector,
  ManiaSimilarityBeatmap,
  ManiaSimilarityResult,
  ManiaStyleVector,
  SimilarityBeatmap,
  SimilarityDynamicWeightProfile,
  SimilarityResult,
} from "../../shared/types/osu";
import { DynamicWeightProfileCard } from "./DynamicWeightProfile";
import { maniaSkillProfile } from "./maniaDifficulty";
import { SimilarityRadar } from "./SimilarityRadar";

const difficultyDimensions = [
  ["aim", "Aim"],
  ["speed", "Speed"],
  ["reading", "Reading"],
  ["slider", "Slider"],
  ["overlap", "Overlap"],
] as const;

const maniaDifficultyDimensions: Array<[keyof ManiaDifficultyVector, string]> = [
  ["speed", "Speed"],
  ["hand_stream", "Hand Stream"],
  ["jack", "Jack"],
  ["chordjack", "Chordjack"],
  ["technical", "Technical"],
  ["stamina", "Stamina"],
  ["long_note", "Long Note"],
  ["course", "Course"],
];

const maniaPatternDimensions: Array<[keyof ManiaStyleVector, string]> = [
  ["stream", "Stream"],
  ["chordstream", "Chordstream"],
  ["jacks", "Jacks"],
  ["coordination", "Coordination"],
  ["density", "Density"],
  ["wildcard", "Wildcard"],
];

const maniaStructureDimensions: Array<[keyof ManiaStyleVector, string]> = [
  ["chord_rate", "Chord Rate"],
  ["large_chord_rate", "Large Chord"],
  ["rotation_rate", "Rotation"],
  ["anchor_rate", "Anchor"],
  ["rhythm_entropy", "Rhythm Entropy"],
  ["transition_entropy", "Transition Entropy"],
  ["ln_note_ratio", "LN Ratio"],
  ["hold_occupancy", "Hold Occupancy"],
  ["hybrid_row_ratio", "Hybrid Row"],
  ["peak_to_sustain_gap", "Peak/Sustain Gap"],
];

const distanceDimensions = [
  ["skill", "强度"],
  ["pattern", "键型"],
  ["structure", "结构"],
  ["difficulty", "难度分位"],
  ["context", "BPM / 有效时长"],
] as const;

function Difference({ value, digits }: { value: number; digits: number }) {
  const tone = value === 0 ? "text-slate-500" : value > 0 ? "text-rose-300" : "text-emerald-300";
  return <small className={`ml-2 ${tone}`}>{value >= 0 ? "+" : ""}{value.toFixed(digits)}</small>;
}

function FeatureRows<T extends object>({
  dimensions,
  selected,
  target,
}: {
  dimensions: Array<[keyof T, string]>;
  selected: T;
  target: T;
}) {
  return dimensions.map(([key, label]) => {
    const selectedValue = Number(selected[key]);
    const targetValue = Number(target[key]);
    const difference = selectedValue - targetValue;
    return (
      <div className="flex items-center justify-between border-b border-white/[0.055] py-1.5 text-xs last:border-b-0" key={String(key)}>
        <span className="text-slate-400">{label}</span>
        <span className="font-mono text-slate-200">{selectedValue.toFixed(3)}<Difference digits={3} value={difference} /></span>
      </div>
    );
  });
}

function StandardComparison({
  selected,
  target,
  recommendedBy,
  dynamicProfile,
  onOpen,
}: {
  selected: SimilarityResult;
  target: SimilarityBeatmap;
  recommendedBy: SimilarityBeatmap | null;
  dynamicProfile: SimilarityDynamicWeightProfile | null;
  onOpen: () => void;
}) {
  return (
    <aside className="sticky top-[120px] self-start">
      <Card className="similarity-comparison-panel min-h-[520px] resize-y overflow-hidden p-5">
        <span className="text-[10px] font-bold uppercase tracking-[0.18em] text-[var(--theme-primary)]">特征对比</span>
        <h2 className="mt-2 text-base font-semibold text-white">{selected.version}</h2>
        <p className="mt-1 truncate text-xs text-slate-400">{selected.artist} — {selected.title}</p>
        {recommendedBy ? <p className="mt-2 text-xs text-cyan-200">由 {recommendedBy.artist} - {recommendedBy.title} [{recommendedBy.version}] 推荐</p> : null}
        <SimilarityRadar target={target.difficulty} comparison={selected.difficulty} />
        {dynamicProfile ? <DynamicWeightProfileCard compact profile={dynamicProfile} /> : null}
        <div className="mb-4 space-y-1.5">
          {difficultyDimensions.map(([key, label]) => {
            const difference = selected.difficulty[key] - target.difficulty[key];
            return <div className="flex items-center justify-between border-b border-white/[0.055] py-1.5 text-xs last:border-b-0" key={key}><span className="text-slate-400">{label}</span><span className="font-mono text-slate-200">{selected.difficulty[key].toFixed(3)}<Difference digits={3} value={difference} /></span></div>;
          })}
        </div>
        <div className="mb-4 space-y-1.5 border-t border-white/[0.07] pt-3">
          {(["ar", "cs", "od"] as const).map((key) => {
            const difference = selected.base[key] - target.base[key];
            return <div className="flex items-center justify-between border-b border-white/[0.055] py-1.5 text-xs last:border-b-0" key={key}><span className="text-slate-400">{key.toUpperCase()}</span><span className="font-mono text-slate-200">{target.base[key].toFixed(1)} → {selected.base[key].toFixed(1)}<Difference digits={1} value={difference} /></span></div>;
          })}
        </div>
        <Button className="w-full" variant="primary" type="button" onClick={onOpen}>在在线谱面中查看</Button>
      </Card>
    </aside>
  );
}

function ManiaComparison({
  selected,
  target,
  recommendedBy,
  onOpen,
}: {
  selected: ManiaSimilarityResult;
  target: ManiaSimilarityBeatmap;
  recommendedBy: ManiaSimilarityBeatmap | null;
  onOpen: () => void;
}) {
  const targetSkillProfile = maniaSkillProfile(target.difficulty);
  const selectedSkillProfile = maniaSkillProfile(selected.difficulty);
  return (
    <aside className="sticky top-[120px] self-start">
      <Card className="similarity-comparison-panel max-h-[calc(100vh-140px)] overflow-y-auto p-5">
        <span className="text-[10px] font-bold uppercase tracking-[0.18em] text-[var(--theme-primary)]">Mania 特征对比</span>
        <h2 className="mt-2 text-base font-semibold text-white">{selected.key_count}K · {selected.game_mod} · {selected.version}</h2>
        <p className="mt-1 truncate text-xs text-slate-400">{selected.artist} — {selected.title}</p>
        {recommendedBy ? <p className="mt-2 text-xs text-cyan-200">由 {recommendedBy.artist} - {recommendedBy.title} [{recommendedBy.version}] 推荐</p> : null}
        <SimilarityRadar target={target.difficulty} comparison={selected.difficulty} />

        <section className="mb-4">
          <p className="mb-1 text-[10px] font-bold uppercase tracking-[0.16em] text-slate-500">八维相对强项</p>
          <FeatureRows dimensions={maniaDifficultyDimensions} selected={selectedSkillProfile} target={targetSkillProfile} />
        </section>
        <section className="mb-4 border-t border-white/[0.07] pt-3">
          <p className="mb-1 text-[10px] font-bold uppercase tracking-[0.16em] text-slate-500">键型分布</p>
          <FeatureRows dimensions={maniaPatternDimensions} selected={selected.style} target={target.style} />
        </section>
        <section className="mb-4 border-t border-white/[0.07] pt-3">
          <p className="mb-1 text-[10px] font-bold uppercase tracking-[0.16em] text-slate-500">结构特征</p>
          <FeatureRows dimensions={maniaStructureDimensions} selected={selected.style} target={target.style} />
        </section>
        <section className="mb-4 border-t border-white/[0.07] pt-3">
          <p className="mb-1 text-[10px] font-bold uppercase tracking-[0.16em] text-slate-500">距离分量</p>
          {distanceDimensions.map(([key, label]) => <div className="flex items-center justify-between border-b border-white/[0.055] py-1.5 text-xs last:border-b-0" key={key}><span className="text-slate-400">{label}</span><span className="font-mono text-slate-200">{selected.distance_components[key].toFixed(4)}</span></div>)}
        </section>
        <Button className="w-full" variant="primary" type="button" onClick={onOpen}>在在线谱面中查看</Button>
      </Card>
    </aside>
  );
}

export function SimilarityComparisonPanel({
  selected,
  target,
  recommendedBy,
  dynamicProfile,
  onOpen,
}: {
  selected: AnySimilarityResult;
  target: AnySimilarityBeatmap;
  recommendedBy: AnySimilarityBeatmap | null;
  dynamicProfile: SimilarityDynamicWeightProfile | null;
  onOpen: () => void;
}) {
  if (selected.ruleset === "mania" && target.ruleset === "mania") {
    return (
      <ManiaComparison
        selected={selected}
        target={target}
        recommendedBy={recommendedBy?.ruleset === "mania" ? recommendedBy : null}
        onOpen={onOpen}
      />
    );
  }
  if (selected.ruleset === "osu" && target.ruleset === "osu") {
    return (
      <StandardComparison
        selected={selected}
        target={target}
        recommendedBy={recommendedBy?.ruleset === "osu" ? recommendedBy : null}
        dynamicProfile={dynamicProfile}
        onOpen={onOpen}
      />
    );
  }
  return null;
}
