import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { Ellipsis, X } from "lucide-react";
import type { OnlineBeatmapset } from "../../shared/types/osu";
import { useIsPresent } from "motion/react";
import { OnlineBeatmapSortBar } from "./OnlineBeatmapSortBar";
import { OnlineResultCard } from "./OnlineResultCard";
import { RESULT_CARD_HEIGHT, RESULT_GAP, resultLayout, visibleResultRows } from "./resultLayout";
import { scrollWheel } from "./scrollWheel";

export function OnlineResults({ items, selectedId, queuedIds, total, sort, variant, busy, loading, error, hasMore, onSort, onRandom, onChoose, onDownload, onLoadMore, onRetry, onAdd, onCollect, collecting, title, playingId, onPreview }: {
  title: string; playingId: number | null; onPreview: (set: OnlineBeatmapset) => void;
  items: OnlineBeatmapset[]; selectedId?: number; queuedIds: Set<number>; total: number | null; sort: string; variant: "grid" | "sidebar"; busy: boolean; loading: boolean; error: string | null; hasMore: boolean;
  onSort: (sort: string) => void; onRandom: () => void; onChoose: (set: OnlineBeatmapset) => void; onDownload: (set: OnlineBeatmapset) => void; onLoadMore: () => void; onRetry: () => void;
  onAdd: (items: OnlineBeatmapset[]) => void; onCollect: (limit: number) => void; collecting: boolean;
}) {
  const expanded = variant === "grid";
  const present = useIsPresent();
  const root = useRef<HTMLDivElement>(null);
  const panelRef = useRef<HTMLElement>(null);
  useEffect(() => {
    const panel = panelRef.current;
    const wheel = (event: WheelEvent) => {
      const target = event.target as Element;
      if (root.current && !root.current.contains(target) && !target.closest('.online-sort-inline, details[open]')) scrollWheel(event, root.current, null);
    };
    panel?.addEventListener("wheel", wheel, { passive: false });
    return () => panel?.removeEventListener("wheel", wheel);
  }, []);
  const [viewport, setViewport] = useState({ top: 0, height: 600, width: 380 });
  const [multi, setMulti] = useState(false);
  const [checked, setChecked] = useState<Set<number>>(() => new Set());
  const [limit, setLimit] = useState(100);
  const positions = useRef({ list: 0, grid: 0 });
  const previousExpanded = useRef(expanded);
  const pendingFocus = useRef<number | null>(null);
  const batchMenu = useRef<HTMLDetailsElement>(null);
  useEffect(() => {
    const close = (event: PointerEvent) => { if (batchMenu.current && !batchMenu.current.contains(event.target as Node)) batchMenu.current.open = false; };
    document.addEventListener("pointerdown", close);
    return () => document.removeEventListener("pointerdown", close);
  }, []);
  useLayoutEffect(() => {
    const node = root.current;
    if (!node) return;
    if (previousExpanded.current !== expanded) {
      positions.current[expanded ? "list" : "grid"] = node.scrollTop;
      node.scrollTop = positions.current[expanded ? "grid" : "list"];
      previousExpanded.current = expanded;
    }
    const measure = () => setViewport({ top: node.scrollTop, height: node.clientHeight || 600, width: Math.max(1, (node.clientWidth || 380) - 20) });
    measure();
    const observer = new ResizeObserver(measure); observer.observe(node);
    return () => observer.disconnect();
  }, [expanded]);
  const columns = expanded ? Math.max(1, Math.floor((viewport.width + RESULT_GAP) / (360 + RESULT_GAP))) : 1;
  const [measuredHeights, setMeasuredHeights] = useState<Record<typeof variant, ReadonlyMap<number, number>>>(() => ({ grid: new Map(), sidebar: new Map() }));
  const heights = measuredHeights[variant];
  const onHeight = useCallback((id: number, height: number) => setMeasuredHeights((current) => {
    if ((current[variant].get(id) ?? RESULT_CARD_HEIGHT) === height) return current;
    const next = new Map(current[variant]); next.set(id, height); return { ...current, [variant]: next };
  }), [variant]);
  const layout = useMemo(() => resultLayout(items.map((item) => item.id), columns, heights), [items, columns, heights]);
  const { start, end } = visibleResultRows(layout.rows, viewport.top, viewport.height);
  useEffect(() => {
    if (present && hasMore && !loading && !error && (viewport.top + viewport.height >= layout.totalHeight - 250)) onLoadMore();
  }, [present, hasMore, loading, error, viewport, layout.totalHeight, onLoadMore]);
  useLayoutEffect(() => {
    if (pendingFocus.current === null) return;
    root.current?.querySelector<HTMLButtonElement>(`[data-result-index="${pendingFocus.current}"]`)?.focus({ preventScroll: true });
    pendingFocus.current = null;
  }, [start, end, viewport]);
  const toggle = (id: number) => setChecked((current) => { const next = new Set(current); if (next.has(id)) next.delete(id); else next.add(id); return next; });
  return <section ref={panelRef} className={`online-results ${expanded ? "is-expanded" : ""}`} data-page-guide-online-results="true" aria-label={title}>
    <header className="online-results-header">
      <div className="online-results-label"><strong>{title}</strong><small>{items.length}{total !== null ? ` / ${total}` : ""}</small></div>
      <OnlineBeatmapSortBar compact sort={sort} onChange={onSort} />
      <details className="online-batch-menu" ref={batchMenu} onKeyDown={(event) => { if (event.key === "Escape" && batchMenu.current) { batchMenu.current.open = false; batchMenu.current.querySelector("summary")?.focus(); } }}>
        <summary aria-label="更多结果操作" title="更多结果操作"><Ellipsis /></summary>
        <div><button onClick={() => { setMulti(!multi); setChecked(new Set()); if (batchMenu.current) batchMenu.current.open = false; }}>{multi ? "退出多选" : "多选"}</button>
          <button disabled={!items.length} onClick={() => { onRandom(); if (batchMenu.current) batchMenu.current.open = false; }}>随机一首</button>
          <p>批量加入 · 当前筛选前 N 首</p><select aria-label="批量收集数量" value={limit} onChange={(event) => setLimit(Number(event.target.value))}>{[50, 100, 250, 500].map((value) => <option key={value} value={value}>{value} 首</option>)}</select>
          <button disabled={collecting} onClick={() => { onCollect(limit); if (batchMenu.current) batchMenu.current.open = false; }}>{collecting ? "正在收集…" : `加入前 ${limit} 首`}</button>
        </div>
      </details>
    </header>
    <div className="online-results-scroll" ref={root} onScroll={(event) => { const top = event.currentTarget.scrollTop; setViewport((current) => ({ ...current, top })); }}>
      <div className="online-virtual-space" role="list" aria-label="谱面集" style={{ height: layout.totalHeight }}>
        {items.slice(start * columns, end * columns).map((item, offset) => {
          const index = start * columns + offset;
          const column = index % columns;
          return <OnlineResultCard key={`${item.id}:${variant}`} item={item} index={index} total={total ?? items.length} selected={selectedId === item.id} queued={queuedIds.has(item.id)} busy={busy} playing={playingId === item.id} multi={multi} checked={checked.has(item.id)}
            style={{ top: layout.rows[Math.floor(index / columns)].top, left: `calc(${column * 100 / columns}% + ${column * RESULT_GAP / columns}px)`, width: `calc(${100 / columns}% - ${RESULT_GAP * (columns - 1) / columns}px)` }}
            onHeight={onHeight} onChoose={() => onChoose(item)} onDownload={() => onDownload(item)} onPreview={() => onPreview(item)} onToggle={() => toggle(item.id)} onNavigate={(event) => {
              if (event.altKey || event.ctrlKey || event.metaKey) return;
              let next: number;
              if (event.key === "ArrowDown") next = Math.min(items.length - 1, index + columns);
              else if (event.key === "ArrowUp") next = Math.max(0, index - columns);
              else if (event.key === "ArrowRight") next = Math.min(items.length - 1, index + 1);
              else if (event.key === "ArrowLeft") next = Math.max(0, index - 1);
              else if (event.key === "Home") next = 0;
              else if (event.key === "End") next = items.length - 1;
              else return;
              event.preventDefault(); pendingFocus.current = next;
              const node = root.current;
              if (node) {
                const { top, height } = layout.rows[Math.floor(next / columns)];
                if (top < node.scrollTop) node.scrollTop = top;
                else if (top + height > node.scrollTop + node.clientHeight) node.scrollTop = top + height - node.clientHeight;
                setViewport((current) => ({ ...current, top: node.scrollTop }));
              }
            }} />;
        })}
      </div>
      {!items.length && !loading && !error ? <p className="online-empty">没有匹配谱面，试试更换关键词或放宽筛选。</p> : null}
      {error ? <div className="online-empty" role="alert">{error}<button onClick={onRetry}>重试</button></div> : loading ? <p className="online-empty" role="status">正在加载谱面…</p> : hasMore ? <button className="online-load-more" onClick={onLoadMore}>加载更多</button> : items.length ? <p className="online-empty">已到达结果末尾</p> : null}
    </div>
    {multi ? <footer className="online-multiselect"><span>已选 {checked.size} 首</span><button disabled={!checked.size} onClick={() => { onAdd(items.filter((item) => checked.has(item.id))); setChecked(new Set()); setMulti(false); }}>加入下载清单</button><button aria-label="取消多选" onClick={() => { setMulti(false); setChecked(new Set()); }}><X /></button></footer> : null}
  </section>;
}
