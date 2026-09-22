import type { ManiaPatternView } from "../../shared/types/osu";
import { mmaRadarValues } from "./mmaRadar";

/** 0..1 占比转百分比；缺记录时返回 null，避免把没有键型记录的谱面显示成 0%。 */
function sharePercent(value: number | null | undefined) {
  return typeof value === "number" && Number.isFinite(value) ? Math.round(Math.min(1, Math.max(0, value)) * 1000) / 10 : null;
}

/** RC / LN 音符占比：短按与长按头数量比，来自分析器的 ln_note_ratio。 */
export function MmaNoteShare({ view }: { view: ManiaPatternView }) {
  const ln = sharePercent(view.ln_note_ratio);
  return (
    <div aria-label="RC 与 LN 音符占比" className="space-y-1.5">
      <div className="mma-note-share flex justify-between text-xs font-medium"><span>RC {ln === null ? "—" : `${(100 - ln).toFixed(1)}%`}</span><span>LN {ln === null ? "—" : `${ln.toFixed(1)}%`}</span></div>
      {ln === null ? null : <div className="flex h-2 overflow-hidden rounded bg-white/10"><div className="bg-cyan-400" style={{ width: `${100 - ln}%` }} /><div className="bg-pink-400" style={{ width: `${ln}%` }} /></div>}
    </div>
  );
}

/** 主模式使用分析器自己的分类结果，不跟随覆盖率最大的轴；无键型记录时不渲染。 */
export function MmaPatternPanel({ view, compact = false }: { view: ManiaPatternView; compact?: boolean }) {
  const bars = compact ? view.bars.slice(0, 5) : view.bars;
  const seconds = new Map(mmaRadarValues(view).map((axis) => [axis.dimension, axis.seconds === null ? "—" : `${axis.seconds.toFixed(1)}s`]));
  return (
    <section aria-label="MMA 键型分布" className="space-y-2">
      <p className="text-sm font-semibold text-white">{view.mode_tag} · {view.category}</p>
      <MmaNoteShare view={view} />
      {bars.length ? bars.map((bar) => (
        <div className="space-y-1" key={bar.pattern}>
          <div className="flex justify-between gap-2 text-xs"><span>{bar.pattern}</span><span className="text-slate-400">{seconds.get(bar.pattern) ?? "—"}</span></div>
          <div aria-label={`${bar.pattern} 相对量`} aria-valuemax={100} aria-valuemin={0} aria-valuenow={Math.round(bar.relative * 100)} className="h-1.5 overflow-hidden rounded bg-white/10" role="meter"><div className="h-full rounded bg-cyan-400" style={{ width: `${bar.relative * 100}%` }} /></div>
          <p className="text-[11px] text-slate-400">{bar.specific_types.map(([name, ratio]) => `${name} (${(ratio * 100).toFixed(1)}%)`).join(", ") || "—"}</p>
        </div>
      )) : <p className="text-xs text-slate-400">未识别到稳定模式</p>}
    </section>
  );
}
