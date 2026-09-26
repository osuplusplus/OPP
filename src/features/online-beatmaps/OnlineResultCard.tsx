import { useCallback, useEffect, useId, useLayoutEffect, useRef, useState, type CSSProperties, type KeyboardEventHandler } from "react";
import { ChevronDown, Download, Headphones, Pause } from "lucide-react";
import type { OnlineBeatmapset } from "../../shared/types/osu";
import { DifficultyIcon } from "../../shared/components/DifficultyIcon";
import { useIsPresent } from "motion/react";
import { OnlineDifficultyPopover } from "./OnlineDifficultyPopover";
import { normalizePreviewUrl, starRange } from "./filters";
import { displayArtist, displayTitle } from "./stageModel";
import { LocalSetBadge } from "./localPresence";
import type { PresenceMap } from "./localPresenceModel";
import { OnlineStatusBadge } from "./OnlineStatusBadge";

export function OnlineResultCard({ presence, item, index, total, selected, queued, busy, playing, multi, checked, style, onChoose, onDownload, onPreview, onToggle, onNavigate, onHeight }: {
  presence?: PresenceMap; item: OnlineBeatmapset; index: number; total: number; selected: boolean; queued: boolean; busy: boolean; playing: boolean; multi: boolean; checked: boolean; style: CSSProperties;
  onChoose: (beatmapId?: number) => void; onDownload: () => void; onPreview: () => void; onToggle: () => void; onNavigate: KeyboardEventHandler<HTMLButtonElement>; onHeight: (id: number, height: number) => void;
}) {
  const root = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const detailId = useId();
  const present = useIsPresent();
  const close = useCallback(() => {
    if (document.getElementById(detailId)?.contains(document.activeElement)) root.current?.querySelector<HTMLButtonElement>(".online-result-difficulty-toggle")?.focus({ preventScroll: true });
    setOpen(false);
  }, [detailId]);
  useEffect(() => {
    if (!open) return;
    const dismiss = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!root.current?.contains(target) && !document.getElementById(detailId)?.contains(target)) close();
    };
    document.addEventListener("pointerdown", dismiss);
    return () => document.removeEventListener("pointerdown", dismiss);
  }, [close, detailId, open]);
  const title = displayTitle(item);
  const difficulties = [...(item.beatmaps ?? [])].sort((a, b) => a.difficulty_rating - b.difficulty_rating || a.id - b.id);
  const cover = item.covers?.cover ?? item.covers?.["cover@2x"] ?? item.covers?.card;

  useLayoutEffect(() => {
    const node = root.current;
    if (!node) return;
    const measure = () => { const height = node.offsetHeight; if (height > 0) onHeight(item.id, height); };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(node);
    return () => observer.disconnect();
  }, [item.id, onHeight, multi]);

  return <div ref={root} role="listitem" aria-setsize={total} aria-posinset={index + 1} className={`online-result-row ${selected ? "is-selected" : ""}`} style={style}
    onKeyDown={(event) => { if (event.key === "Escape") close(); }}>
    <div className="online-result-summary">
      {multi ? <input type="checkbox" aria-label={`选择 ${title}`} checked={checked} disabled={item.availability?.download_disabled} onChange={onToggle} /> : null}
      <button className="online-result-main" data-result-index={index} aria-label={`查看 ${title}`} aria-current={selected ? "true" : undefined} onClick={() => onChoose()} onKeyDown={onNavigate}>
        <span className="online-result-cover">{cover ? <img src={cover} alt="" loading="lazy" onError={(event) => { event.currentTarget.style.visibility = "hidden"; }} /> : null}</span>
        <span className="online-result-copy"><strong title={title}>{title}</strong><span title={displayArtist(item)}>{displayArtist(item)}</span><small title={`谱师 ${item.creator}`}>谱师 {item.creator}</small></span>
      </button>
    </div>
    <div className="online-result-meta"><OnlineStatusBadge status={item.status} /><LocalSetBadge set={item} presence={presence} /><span>{starRange(item.beatmaps)}</span>{queued ? <small>已在清单</small> : null}{item.availability?.download_disabled ? <small>禁止下载</small> : null}</div>
    <div className="online-result-bottom">
      <button className="online-result-difficulty-toggle" aria-label={`${difficulties.length} 个难度`} title="点击展开难度详情" aria-expanded={open} aria-controls={detailId} onClick={() => setOpen((value) => !value)}>
        <span className="online-result-difficulty-icons">{difficulties.slice(0, 6).map((difficulty) => <DifficultyIcon key={difficulty.id} mode={difficulty.mode} stars={difficulty.difficulty_rating} showValue={false} className="online-result-difficulty-icon" />)}{difficulties.length > 6 ? <i>+{difficulties.length - 6}</i> : null}</span><ChevronDown className={open ? "is-open" : ""} />
      </button>
      <div className="online-result-actions"><button aria-label={`${playing ? "暂停试听" : "试听"} ${title}`} title={playing ? "暂停试听" : "试听"} disabled={!normalizePreviewUrl(item.preview_url)} onClick={onPreview}>{playing ? <Pause /> : <Headphones />}</button><button aria-label={`下载 ${title}`} title="下载" disabled={busy || item.availability?.download_disabled} onClick={onDownload}><Download /></button></div>
    </div>
    {open && present ? <OnlineDifficultyPopover presence={presence} anchor={root} item={item} id={detailId} onClose={close} onChoose={(beatmapId) => onChoose(beatmapId)} /> : null}
  </div>;
}
