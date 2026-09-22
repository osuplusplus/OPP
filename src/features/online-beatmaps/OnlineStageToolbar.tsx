import * as Dialog from "@radix-ui/react-dialog";
import { useId, useRef, useState } from "react";
import { Search, SlidersHorizontal, X } from "lucide-react";
import type { OnlineBeatmapSearchQuery, OnlineBeatmapset, Ruleset } from "../../shared/types/osu";
import { OnlineBeatmapFilters } from "./OnlineBeatmapFilters";
import { clearOnlineFilters, filterChips } from "./stageModel";

function FilterEditor({ query, ruleset, onApply }: { query: OnlineBeatmapSearchQuery; ruleset: Ruleset; onApply: (query: OnlineBeatmapSearchQuery) => void }) {
  const [draft, setDraft] = useState(query);
  return <OnlineBeatmapFilters submitLabel="应用并搜索" loading={false} query={draft} onChange={setDraft} onReset={() => setDraft(clearOnlineFilters(draft, ruleset))} onSubmit={onApply} />;
}

export function OnlineStageToolbar({ compact = false, query, ruleset, items, text, onTextChange, onApply, onClear }: {
  compact?: boolean;
  query: OnlineBeatmapSearchQuery; ruleset: Ruleset; items: OnlineBeatmapset[]; text: string; onTextChange: (text: string) => void;
  onApply: (query: OnlineBeatmapSearchQuery) => void; onClear: () => void;
}) {
  const [filtersOpen, setFiltersOpen] = useState(false);
  const suggestionsId = useId();
  const inputRef = useRef<HTMLInputElement>(null);
  const [suggestionsEnabled, setSuggestionsEnabled] = useState(true);
  const dismissSuggestions = () => { setSuggestionsEnabled(false); inputRef.current?.blur(); };
  const chips = filterChips(query);
  const suggestions = [...new Set(items.flatMap((item) => [item.title, item.title_unicode, item.artist, item.creator]).filter(Boolean))].slice(0, 60);
  return <Dialog.Root open={filtersOpen} onOpenChange={setFiltersOpen}>
    <form className="online-search beatmap-search-field" data-compact={compact} data-page-guide-online-search="true" onSubmit={(event) => { event.preventDefault(); dismissSuggestions(); onApply({ ...query, query: text.trim(), sort: text.trim() ? "relevance_desc" : "ranked_desc" }); }}>
      <Search /><input ref={inputRef} aria-label="搜索在线谱面" list={suggestionsId} onFocus={() => setSuggestionsEnabled(true)} value={text} placeholder="搜索曲名、艺术家、谱师或标签…" onChange={(event) => { setSuggestionsEnabled(true); onTextChange(event.target.value); }} onKeyDown={(event) => { if (event.key === "Enter" && (event.nativeEvent.isComposing || event.keyCode === 229)) event.preventDefault(); }} />
      <datalist id={suggestionsId}>{(suggestionsEnabled ? suggestions : []).map((value) => <option key={value} value={value} />)}</datalist>
      <button type="button" aria-label="清空搜索" title="清空搜索" disabled={!text && !query.query} onClick={() => { dismissSuggestions(); onClear(); }}><X /></button>
      <Dialog.Trigger asChild><button type="button" aria-label={`筛选 ${chips.length}`} data-page-guide-online-filter-trigger="true"><SlidersHorizontal /><span>筛选</span>{chips.length ? <small>{chips.length}</small> : null}</button></Dialog.Trigger>
    </form>
    <Dialog.Portal><Dialog.Overlay className="online-dialog-overlay" /><Dialog.Content data-dialog-layout="filters" className="online-dialog online-filter-dialog">
      <Dialog.Title>筛选在线谱面</Dialog.Title><Dialog.Description>调整关键词与筛选条件后统一搜索；关闭时舍弃未应用的修改。</Dialog.Description><Dialog.Close className="online-dialog-close" aria-label="关闭筛选"><X /></Dialog.Close>
      <FilterEditor query={{ ...query, query: text }} ruleset={ruleset} onApply={(next) => { setFiltersOpen(false); dismissSuggestions(); onApply(next); }} />
    </Dialog.Content></Dialog.Portal>
  </Dialog.Root>;
}
