import { useLayoutEffect, useRef, type RefObject } from "react";
import type { OnlineBeatmapset } from "../../shared/types/osu";
import { OnlineResultCard } from "./OnlineResultCard";
import { useOnlineLocalPresence } from "./useOnlineLocalPresence";
import { TREND_WINDOW_DAYS } from "./stageModel";

const ignoreHeight = () => undefined;
export function OnlineHome({ items, loading, error, busy, playingId, queuedIds, onChoose, onPreview, onDownload, onRetry, scrollPositionRef }: {
  scrollPositionRef: RefObject<number>;
  items: OnlineBeatmapset[]; loading: boolean; error: string | null; busy: boolean; playingId: number | null; queuedIds: Set<number>;
  onChoose: (item: OnlineBeatmapset, beatmapId?: number) => void; onPreview: (item: OnlineBeatmapset) => void; onDownload: (item: OnlineBeatmapset) => void; onRetry: () => void;
}) {
  const root = useRef<HTMLElement>(null);
  const presence = useOnlineLocalPresence(items);
  useLayoutEffect(() => { if (root.current) root.current.scrollTop = scrollPositionRef.current; }, [scrollPositionRef]);
  return <section ref={root} onScroll={(event) => { scrollPositionRef.current = event.currentTarget.scrollTop; }} className="online-home-scroll" aria-label="近期热门谱面">
    <header><h2>TREND <span>· 近期热门</span></h2><p>近 {TREND_WINDOW_DAYS} 天上架 · 按收藏量</p></header>
    <div className="online-trend-list" role="list" aria-label="近期热门">
      {items.map((item, index) => <OnlineResultCard presence={presence} key={item.id} item={item} index={index} total={items.length} selected={false} queued={queuedIds.has(item.id)} busy={busy} playing={playingId === item.id} multi={false} checked={false} style={{ position: "relative" }} onHeight={ignoreHeight} onChoose={(beatmapId) => onChoose(item, beatmapId)} onPreview={() => onPreview(item)} onDownload={() => onDownload(item)} onToggle={() => undefined} onNavigate={(event) => {
        const direction = event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
        if (!direction) return;
        event.preventDefault(); event.currentTarget.closest("[role=list]")?.querySelector<HTMLButtonElement>(`[data-result-index="${Math.max(0, Math.min(items.length - 1, index + direction))}"]`)?.focus();
      }} />)}
    </div>
    {loading ? <div className="online-home-skeleton" role="status">正在加载近期热门…</div> : error ? <p className="online-empty" role="alert">近期热门暂时无法加载。<button onClick={onRetry}>重试热门</button></p> : !items.length ? <p className="online-empty">近期暂无热门谱面，试试搜索喜欢的曲名。</p> : null}
  </section>;
}
