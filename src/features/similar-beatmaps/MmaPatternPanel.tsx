import type { ManiaPatternView } from "../../shared/types/osu";

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
      {ln === null ? null : <div className="flex h-2 overflow-hidden rounded bg-white/10"><div className="mma-share-rc" style={{ width: `${100 - ln}%` }} /><div className="mma-share-ln" style={{ width: `${ln}%` }} /></div>}
    </div>
  );
}

/** 与 Mania Map Analyser 的 SV_AMOUNT_THRESHOLD 一致：变速总量达到它就标记为 SV 谱。 */
const SV_AMOUNT_THRESHOLD = 2000;

/** 主模式使用分析器自己的分类结果，不跟随覆盖率最大的轴；无键型记录时不渲染。 */
export function MmaPatternPanel({ view, compact = false }: { view: ManiaPatternView; compact?: boolean }) {
  const bars = compact ? view.bars.slice(0, 5) : view.bars;
  const isSv = view.sv_amount >= SV_AMOUNT_THRESHOLD;
  return (
    <section aria-label="MMA 键型分布" className="space-y-2">
      <p className="flex flex-wrap items-center gap-1.5">
        <span className={`mma-mode-tag mma-mode-${view.mode_tag.toLowerCase()}`}>{view.mode_tag}</span>
        {isSv ? <span className="mma-mode-tag mma-mode-sv">SV</span> : null}
        {/* 变速谱的分类名本身就是 SV，标签已经表达了，不再重复。 */}
        {isSv && view.category === "SV" ? null : <strong className="text-base font-semibold text-white">{view.category}</strong>}
      </p>
      <MmaNoteShare view={view} />
      {bars.length ? bars.map((bar) => (
        <div className="space-y-1" key={bar.pattern}>
          <p className="text-[13px] font-semibold">{bar.pattern}</p>
          <div aria-label={`${bar.pattern} 相对量`} aria-valuemax={100} aria-valuemin={0} aria-valuenow={Math.round(bar.relative * 100)} className="h-2 overflow-hidden rounded-full bg-white/10" role="meter"><div className="mma-bar-fill h-full rounded-full" style={{ width: `${bar.relative * 100}%` }} /></div>
          <p className="text-[11px] text-slate-400">{bar.specific_types.map(([name, ratio]) => `${name} (${(ratio * 100).toFixed(1)}%)`).join(", ") || "—"}</p>
        </div>
      )) : <p className="text-xs text-slate-400">未识别到稳定模式</p>}
    </section>
  );
}
