import { useDeferredValue, useEffect, useId, useRef, useState } from "react";
import { ChevronLeft, ChevronRight, Search, SlidersHorizontal, X } from "lucide-react";
import type { BeatmapQuery, BeatmapSort, LocalBeatmapSetSummary } from "../../shared/types/osu";
import { fixedNumber } from "../../shared/lib/format";
import { useLocalBeatmapBackground, useLocalBeatmapSets } from "./api";
import { rangeFields } from "./stageModel";

function Candidate({ set, client }: { set: LocalBeatmapSetSummary; client: BeatmapQuery["client"] }) {
  const image = useLocalBeatmapBackground(client, set.background_resource_id);
  return <>
    <span className="local-search-cover">{image.data ? <img src={image.data} alt="" /> : <Search />}</span>
    <span className="local-search-copy"><strong>{set.title_unicode || set.title}</strong><span>{set.artist_unicode || set.artist} · {set.creators.join(" / ")}</span><small>{set.difficulties.length} 个匹配难度 · {fixedNumber(set.min_stars, 2)}–{fixedNumber(set.max_stars, 2)} ★ · {set.difficulties.map((d) => d.difficulty_name).join(" / ")}</small></span>
  </>;
}

export function LocalSetSearch({ query, onQuery, onChoose }: { query: BeatmapQuery; onQuery: (query: BeatmapQuery) => void; onChoose: (set: LocalBeatmapSetSummary) => void }) {
  const [open, setOpen] = useState(false);
  const [filters, setFilters] = useState(false);
  const [highlighted, setHighlighted] = useState(0);
  const root = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const listId = useId();
  const deferred = useDeferredValue(query);
  const result = useLocalBeatmapSets(deferred, open);
  const items = result.data?.items ?? [];
  const stale = query !== deferred || result.isPlaceholderData || result.isFetching;
  const active = Math.min(highlighted, Math.max(0, items.length - 1));
  const update = (patch: Partial<BeatmapQuery>) => { onQuery({ ...query, ...patch, offset: patch.offset ?? 0 }); setHighlighted(0); setOpen(true); };
  const choose = (set: LocalBeatmapSetSummary) => { if (stale) return; setOpen(false); setFilters(false); onChoose(set); };
  const filterCount = rangeFields.filter(([key]) => query[key] != null).length + Number(query.submitted != null);
  useEffect(() => {
    const close = (event: PointerEvent) => { if (!root.current?.contains(event.target as Node)) { setOpen(false); setFilters(false); } };
    const shortcut = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k" && !document.querySelector('[role="dialog"]')) {
        event.preventDefault(); input.current?.focus(); setOpen(true);
      }
    };
    document.addEventListener("pointerdown", close); document.addEventListener("keydown", shortcut);
    return () => { document.removeEventListener("pointerdown", close); document.removeEventListener("keydown", shortcut); };
  }, []);
  useEffect(() => {
    if (open) document.getElementById(`${listId}-${active}`)?.scrollIntoView?.({ block: "nearest" });
  }, [active, open, listId]);
  return <div className="local-set-search" ref={root} onKeyDown={(event) => { if (event.key === "Escape" && !event.nativeEvent.isComposing) { event.stopPropagation(); input.current?.focus(); setOpen(false); setFilters(false); } }} onBlur={(event) => { if (!event.currentTarget.contains(event.relatedTarget)) { setOpen(false); setFilters(false); } }}>
    <div className="local-search-field"><Search aria-hidden="true" />
      <input ref={input} aria-label="搜索谱面集" role="combobox" aria-autocomplete="list" aria-expanded={open} aria-controls={open ? listId : undefined} aria-activedescendant={open && items.length && !stale ? `${listId}-${active}` : undefined}
        value={query.search} placeholder="搜索曲名、曲师、谱师、难度、来源、标签或 ID…" onFocus={() => setOpen(true)} onClick={() => setOpen(true)} onChange={(event) => update({ search: event.target.value })}
        onKeyDown={(event) => {
          if (event.nativeEvent.isComposing || event.keyCode === 229) return;
          if (event.key === "Escape") { setOpen(false); setFilters(false); return; }
          if (event.key === "ArrowDown" || event.key === "ArrowUp") {
            event.preventDefault(); setOpen(true);
            if (!stale && items.length) setHighlighted((active + (event.key === "ArrowDown" ? 1 : -1) + items.length) % items.length);
          }
          if (event.key === "Enter" && open && items[active] && !stale) { event.preventDefault(); choose(items[active]); }
        }} />
      {query.search ? <button type="button" aria-label="清空搜索" onClick={() => { update({ search: "" }); input.current?.focus(); }}><X /></button> : <kbd>Ctrl K</kbd>}
      <button type="button" aria-label="谱面筛选" aria-expanded={filters} className={filterCount ? "is-active" : ""} onClick={() => { setFilters(!filters); setOpen(true); }}><SlidersHorizontal /><span>筛选{filterCount ? ` ${filterCount}` : ""}</span></button>
    </div>
    {open ? <div className="local-search-menu">
      {filters ? <div className="local-search-filters">
        <div className="local-filter-selects">
          <label>来源<select aria-label="谱面集来源" value={query.submitted == null ? "all" : query.submitted ? "online" : "local"} onChange={(e) => update({ submitted: e.target.value === "all" ? null : e.target.value === "online" })}><option value="all">全部来源</option><option value="online">已提交</option><option value="local">本地 / 未提交</option></select></label>
          <label>排序<select aria-label="谱面排序" value={query.sort} onChange={(e) => update({ sort: e.target.value as BeatmapSort })}>{[["title", "标题"], ["artist", "艺术家"], ["creator", "谱师"], ["stars", "最高星数"], ["bpm", "BPM"], ["length", "时长"], ["object_count", "物件数"], ["modified_at", "修改时间"]].map(([value, label]) => <option value={value} key={value}>{label}</option>)}</select></label>
          <label>方向<select aria-label="排序方向" value={query.direction} onChange={(e) => update({ direction: e.target.value as "asc" | "desc" })}><option value="asc">升序</option><option value="desc">降序</option></select></label>
        </div>
        <div className="local-star-presets">{[["全部", null, null], ["入门", null, 2.69], ["进阶", 2.7, 3.99], ["高难", 4, 5.29], ["专家", 5.3, 6.49], ["极限", 6.5, null]].map(([label, min, max]) => <button type="button" key={label} aria-pressed={query.min_stars === min && query.max_stars === max} onClick={() => update({ min_stars: min as number | null, max_stars: max as number | null })}>{label}</button>)}</div>
        <div className="local-filter-ranges">{rangeFields.map(([key, label, scale]) => <label key={key}>{label}<input aria-label={label} type="number" min="0" step="0.1" placeholder="不限" value={query[key] == null ? "" : query[key] / scale} onChange={(e) => update({ [key]: e.target.value === "" || !Number.isFinite(e.target.valueAsNumber) ? null : e.target.valueAsNumber * scale })} /></label>)}</div>
        <button type="button" className="local-filter-reset" onClick={() => update({ ...Object.fromEntries(rangeFields.map(([key]) => [key, null])), submitted: null })}>清空筛选</button>
      </div> : null}
      <div className="local-search-meta"><span>{stale ? "正在寻找谱面…" : `${result.data?.total ?? 0} 个谱面集`}</span></div>
      <div id={listId} role="listbox" aria-label="候选谱面集" aria-busy={stale} className="local-search-results">
        {result.error ? <div className="local-search-message" role="alert">搜索失败 <button type="button" onClick={() => void result.refetch()}>重试</button></div> : items.length ? items.map((set, index) => <button type="button" tabIndex={-1} role="option" aria-selected={active === index} aria-label={`${set.title_unicode || set.title} · ${set.artist_unicode || set.artist}`} id={`${listId}-${index}`} key={set.set_key} disabled={stale} onMouseDown={(event) => event.preventDefault()} onPointerMove={() => setHighlighted(index)} onClick={() => choose(set)}><Candidate set={set} client={query.client} /></button>) : <p className="local-search-message">{stale ? "正在读取候选…" : "没有匹配的谱面，试试其他关键词或放宽筛选。"}</p>}
      </div>
      <div className="local-search-footer"><div><button type="button" aria-label="上一页候选" disabled={!query.offset || stale} onClick={() => update({ offset: Math.max(0, query.offset - 20) })}><ChevronLeft /></button><span>{Math.floor(query.offset / 20) + 1} / {Math.max(1, Math.ceil((result.data?.total ?? 0) / 20))}</span><button type="button" aria-label="下一页候选" disabled={stale || query.offset + 20 >= (result.data?.total ?? 0)} onClick={() => update({ offset: query.offset + 20 })}><ChevronRight /></button></div></div>
    </div> : null}
  </div>;
}
