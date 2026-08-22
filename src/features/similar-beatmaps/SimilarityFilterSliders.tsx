import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { RotateCcw, SlidersHorizontal, X } from "lucide-react";

import { Button, Card } from "../../shared/components/ui";
import type { OsuSimilarityQueryRequest, SimilarityFilters } from "../../shared/types/osu";
import { defaultSimilarityFilters } from "./defaults";

interface FilterControl { label: string; minKey: keyof SimilarityFilters; maxKey: keyof SimilarityFilters; floor: number; ceiling: number; step: number; format?: (value: number) => string; }
const controls: FilterControl[] = [
  { label: "星数", minKey: "min_star", maxKey: "max_star", floor: 0, ceiling: 10, step: 0.1, format: (value) => value.toFixed(1) },
  { label: "AR", minKey: "min_ar", maxKey: "max_ar", floor: 0, ceiling: 11, step: 0.1, format: (value) => value.toFixed(1) },
  { label: "CS", minKey: "min_cs", maxKey: "max_cs", floor: 0, ceiling: 10, step: 0.1, format: (value) => value.toFixed(1) },
  { label: "长度", minKey: "min_length_seconds", maxKey: "max_length_seconds", floor: 0, ceiling: 900, step: 5, format: (value) => `${Math.round(value / 60)} 分` },
  { label: "BPM", minKey: "min_bpm", maxKey: "max_bpm", floor: 0, ceiling: 400, step: 1, format: (value) => Math.round(value).toString() },
  { label: "OD", minKey: "min_od", maxKey: "max_od", floor: 0, ceiling: 11, step: 0.1, format: (value) => value.toFixed(1) },
];

function RangeFilter({ control, filters, onChange }: { control: FilterControl; filters: SimilarityFilters; onChange: (filters: SimilarityFilters) => void }) {
  const currentMin = filters[control.minKey] ?? control.floor;
  const currentMax = filters[control.maxKey] ?? control.ceiling;
  const format = control.format ?? String;
  const setMinimum = (value: number) => onChange({ ...filters, [control.minKey]: value === control.floor ? null : Math.min(value, currentMax) });
  const setMaximum = (value: number) => onChange({ ...filters, [control.maxKey]: value === control.ceiling ? null : Math.max(value, currentMin) });
  const setTypedValue = (bound: "min" | "max", rawValue: string) => {
    const key = bound === "min" ? control.minKey : control.maxKey;
    if (rawValue.trim() === "") {
      onChange({ ...filters, [key]: null });
      return;
    }
    const value = Number(rawValue);
    if (!Number.isFinite(value)) return;
    const clamped = Math.max(control.floor, Math.min(control.ceiling, value));
    if (bound === "min") setMinimum(clamped);
    else setMaximum(clamped);
  };
  return (
    <section className="border-t border-[var(--line-subtle)] px-3.5 py-3">
      <div className="mb-2 flex items-center justify-between gap-3"><h3 className="text-xs font-semibold text-slate-200">{control.label}</h3><output className="font-mono text-[11px] text-[var(--theme-primary-light)]">{format(currentMin)} — {format(currentMax)}</output></div>
      <div className="flex items-center gap-2">
        <input aria-label={`${control.label} 最低`} className="opp-filter-number" max={currentMax} min={control.floor} onChange={(event) => setTypedValue("min", event.target.value)} step={control.step} type="number" value={filters[control.minKey] ?? ""} />
        <div className="opp-dual-range">
          <div className="opp-dual-range__track" />
          <div className="opp-dual-range__selection" style={{ left: `${((currentMin - control.floor) / (control.ceiling - control.floor)) * 100}%`, right: `${100 - ((currentMax - control.floor) / (control.ceiling - control.floor)) * 100}%` }} />
          <input aria-label={`${control.label} 最低滑块`} className="opp-dual-range__input opp-dual-range__input--min" max={control.ceiling} min={control.floor} onChange={(event) => setMinimum(Number(event.target.value))} step={control.step} type="range" value={currentMin} />
          <input aria-label={`${control.label} 最高滑块`} className="opp-dual-range__input opp-dual-range__input--max" max={control.ceiling} min={control.floor} onChange={(event) => setMaximum(Number(event.target.value))} step={control.step} type="range" value={currentMax} />
        </div>
        <input aria-label={`${control.label} 最高`} className="opp-filter-number" max={control.ceiling} min={currentMin} onChange={(event) => setTypedValue("max", event.target.value)} step={control.step} type="number" value={filters[control.maxKey] ?? ""} />
      </div>
    </section>
  );
}

export function SimilarityFilterSliders({ request, onChange }: { request: OsuSimilarityQueryRequest; onChange: (request: OsuSimilarityQueryRequest) => void }) {
  const [open, setOpen] = useState(false);
  const [triggerAnimating, setTriggerAnimating] = useState(false);
  const popoverRef = useRef<HTMLDivElement>(null);
  const triggerAnimationTimer = useRef<number | undefined>(undefined);
  const activeCount = Object.entries(request.filters).filter(([key, value]) => value !== defaultSimilarityFilters[key as keyof SimilarityFilters]).length;

  const toggle = () => {
    window.clearTimeout(triggerAnimationTimer.current);
    setTriggerAnimating(false);
    requestAnimationFrame(() => setTriggerAnimating(true));
    triggerAnimationTimer.current = window.setTimeout(() => setTriggerAnimating(false), 240);
    setOpen((value) => !value);
  };

  useEffect(() => () => window.clearTimeout(triggerAnimationTimer.current), []);

  useEffect(() => {
    if (!open) return;
    const closeOnOutsideClick = (event: MouseEvent) => {
      if (popoverRef.current && !popoverRef.current.contains(event.target as Node)) setOpen(false);
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", closeOnOutsideClick);
    window.addEventListener("keydown", closeOnEscape);
    return () => {
      document.removeEventListener("mousedown", closeOnOutsideClick);
      window.removeEventListener("keydown", closeOnEscape);
    };
  }, [open]);

  return createPortal(
    <div ref={popoverRef}>
      <Card className={`opp-candidate-filter-panel !fixed bottom-20 left-[276px] z-[170] w-[360px] max-w-[calc(100vw-304px)] origin-bottom-left overflow-hidden p-0 shadow-xl transition-[opacity,transform] duration-200 ease-out ${open ? "opp-candidate-filter-panel--open translate-y-0 scale-100 opacity-100" : "pointer-events-none translate-y-3 scale-[0.98] opacity-0"}`}>
        <div className="flex items-center justify-between gap-4 border-b border-white/[0.08] px-5 py-3">
          <div><p className="text-sm font-semibold text-slate-200">候选谱面筛选</p><p className="mt-0.5 text-xs text-slate-500">{activeCount ? `已启用 ${activeCount} 项条件` : "未启用条件"}</p></div>
          <div className="flex items-center gap-1">{activeCount ? <Button aria-label="清除筛选" onClick={() => onChange({ ...request, filters: { ...defaultSimilarityFilters } })} size="icon" variant="ghost"><RotateCcw className="size-3.5" /></Button> : null}<Button aria-label="关闭筛选" onClick={() => setOpen(false)} size="icon" variant="ghost"><X className="size-4" /></Button></div>
        </div>
        <div className="opp-filter-body max-h-[calc(100vh-10rem)] overflow-y-auto px-5 pb-4">{controls.map((control) => <RangeFilter control={control} filters={request.filters} key={control.label} onChange={(filters) => onChange({ ...request, filters })} />)}</div>
      </Card>
      <button aria-expanded={open} aria-label={open ? "收起候选谱面筛选" : "打开候选谱面筛选"} className={`opp-candidate-filter-trigger fixed bottom-7 left-[276px] z-[171] grid size-11 place-items-center rounded-lg border border-white/10 bg-[var(--surface-panel)] text-[var(--theme-primary)] shadow-xl transition-colors hover:border-[var(--theme-primary-soft)] hover:bg-[var(--theme-primary-muted)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--theme-primary)] ${triggerAnimating ? "opp-candidate-filter-trigger--animate" : ""}`} onClick={toggle} type="button">
        <SlidersHorizontal className={`size-5 transition-transform duration-200 ${open ? "rotate-90" : ""}`} />
        {activeCount ? <span className="absolute -right-1 -top-1 grid size-4 place-items-center rounded-full bg-[var(--theme-primary)] text-[9px] font-bold text-[var(--on-primary)]">{activeCount > 9 ? "9+" : activeCount}</span> : null}
      </button>
    </div>,
    document.body,
  );
}
