/* eslint-disable react-refresh/only-export-components */
import { useCallback, useEffect, useRef, useState } from "react";
import { RotateCcw, SlidersHorizontal, X } from "lucide-react";
import type { ManiaSimilarityResult } from "../../shared/types/osu";

export interface ManiaCandidateFilterValue {
  minBpm: number | null;
  maxBpm: number | null;
  minLength: number | null;
  maxLength: number | null;
  minPercentile: number | null;
  maxPercentile: number | null;
}

export const defaultManiaCandidateFilters: ManiaCandidateFilterValue = {
  minBpm: null,
  maxBpm: null,
  minLength: null,
  maxLength: null,
  minPercentile: null,
  maxPercentile: null,
};

export function matchesManiaCandidate(result: ManiaSimilarityResult, filters: ManiaCandidateFilterValue) {
  const percentile = result.difficulty_percentile * 100;
  return (filters.minBpm == null || result.base.bpm >= filters.minBpm)
    && (filters.maxBpm == null || result.base.bpm <= filters.maxBpm)
    && (filters.minLength == null || result.base.active_length_seconds >= filters.minLength)
    && (filters.maxLength == null || result.base.active_length_seconds <= filters.maxLength)
    && (filters.minPercentile == null || percentile >= filters.minPercentile)
    && (filters.maxPercentile == null || percentile <= filters.maxPercentile);
}

const fields: Array<{ min: keyof ManiaCandidateFilterValue; max: keyof ManiaCandidateFilterValue; label: string; suffix: string; step: number }> = [
  { min: "minBpm", max: "maxBpm", label: "BPM", suffix: "", step: 1 },
  { min: "minLength", max: "maxLength", label: "有效长度", suffix: " 秒", step: 5 },
  { min: "minPercentile", max: "maxPercentile", label: "难度分位", suffix: "%", step: 1 },
];

export function ManiaCandidateFilters({ value, onChange, total, visible }: { value: ManiaCandidateFilterValue; onChange: (value: ManiaCandidateFilterValue) => void; total: number; visible: number }) {
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState<ManiaCandidateFilterValue>(() => ({ ...value }));
  const root = useRef<HTMLDivElement>(null);
  const count = Object.values(value).filter((item) => item != null).length;
  const draftCount = Object.values(draft).filter((item) => item != null).length;
  const discard = useCallback(() => { setDraft({ ...value }); setOpen(false); }, [value]);
  useEffect(() => {
    if (!open) return;
    const close = (event: PointerEvent) => { if (!root.current?.contains(event.target as Node)) discard(); };
    document.addEventListener("pointerdown", close);
    return () => document.removeEventListener("pointerdown", close);
  }, [discard, open]);
  const setNumber = (key: keyof ManiaCandidateFilterValue, raw: string) => setDraft((current) => ({ ...current, [key]: raw === "" ? null : Number(raw) }));
  return <div className="mania-candidate-filter" ref={root}>
    <button type="button" aria-label={`候选谱面筛选${count ? `，已启用 ${count} 项` : ""}`} aria-expanded={open} className={count ? "is-active" : ""} onClick={() => { if (open) discard(); else { setDraft({ ...value }); setOpen(true); } }}><SlidersHorizontal /><span>筛选{count ? ` ${count}` : ""}</span></button>
    {open ? <div className="mania-candidate-filter-panel">
      <header><div><strong>候选谱面筛选</strong><small>当前 {visible} / {total} · {draftCount ? `待应用 ${draftCount} 项` : "未设置条件"}</small></div><button type="button" aria-label="关闭候选谱面筛选" onClick={discard}><X /></button></header>
      {fields.map((field) => <section key={field.label}><span>{field.label}</span><label>最低<input aria-label={`${field.label} 最低`} type="number" min="0" step={field.step} value={draft[field.min] ?? ""} placeholder="不限" onChange={(event) => setNumber(field.min, event.target.value)} />{field.suffix}</label><label>最高<input aria-label={`${field.label} 最高`} type="number" min="0" step={field.step} value={draft[field.max] ?? ""} placeholder="不限" onChange={(event) => setNumber(field.max, event.target.value)} />{field.suffix}</label></section>)}
      <button className="mania-filter-reset" type="button" disabled={!draftCount} onClick={() => setDraft({ ...defaultManiaCandidateFilters })}><RotateCcw />重置筛选</button>
      <footer className="mania-filter-footer"><button type="button" onClick={discard}>取消</button><button className="is-primary" type="button" onClick={() => { onChange({ ...draft }); setOpen(false); }}>应用筛选</button></footer>
    </div> : null}
  </div>;
}
