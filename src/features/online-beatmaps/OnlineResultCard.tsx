import { useCallback, useEffect, useId, useLayoutEffect, useRef, useState, type CSSProperties, type KeyboardEventHandler } from "react";
import { ChevronDown, Download, Headphones, Pause } from "lucide-react";
import type { OnlineBeatmapset } from "../../shared/types/osu";
import { DifficultyIcon } from "../../shared/components/DifficultyIcon";
import { useIsPresent } from "motion/react";
import { OnlineDifficultyPopover } from "./OnlineDifficultyPopover";
import { normalizePreviewUrl, starRange } from "./filters";
import { displayArtist, displayTitle } from "./stageModel";
import { OnlineStatusBadge } from "./OnlineStatusBadge";

export function OnlineResultCard({ item, index, total, selected, queued, busy, playing, multi, checked, style, onChoose, onDownload, onPreview, onToggle, onNavigate, onHeight }: {
  item: OnlineBeatmapset; index: number; total: number; selected: boolean; queued: boolean; busy: boolean; playing: boolean; multi: boolean; checked: boolean; style: CSSProperties;
  onChoose: () => void; onDownload: () => void; onPreview: () => void; onToggle: () => void; onNavigate: KeyboardEventHandler<HTMLButtonElement>; onHeight: (id: number, height: number) => void;
}) {
  const root = useRef<HTMLDivElement>(null);
  const [hovered, setHovered] = useState(false);
  const [focused, setFocused] = useState(false);
  const [touchOpen, setTouchOpen] = useState(false);
  const [dismissed, setDismissed] = useState(false);
  const touch = useRef(false);
  const open = !dismissed && (hovered || focused || touchOpen);
  const detailId = useId();
  const present = useIsPresent();
  const leaveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const enter = () => { if (leaveTimer.current) clearTimeout(leaveTimer.current); setHovered(true); setDismissed(false); };
  const leave = () => { if (leaveTimer.current) clearTimeout(leaveTimer.current); leaveTimer.current = setTimeout(() => { setHovered(false); setDismissed(false); }, 90); };
  const close = useCallback(() => {
    if (document.getElementById(detailId)?.contains(document.activeElement)) root.current?.querySelector<HTMLButtonElement>(".online-result-difficulty-toggle")?.focus({ preventScroll: true });
    setDismissed(true); setTouchOpen(false);
  }, [detailId]);
  useEffect(() => () => { if (leaveTimer.current) clearTimeout(leaveTimer.current); }, []);
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
    onPointerEnter={(event) => { if (event.pointerType !== "touch") { touch.current = false; enter(); } }}
    onPointerLeave={leave}
    onPointerDown={(event) => { touch.current = event.pointerType === "touch"; setFocused(false); }}
    onFocus={(event) => { if (!touch.current && event.target.matches(":focus-visible")) { setFocused(true); setDismissed(false); } }}
    onBlur={(event) => { if (!event.currentTarget.contains(event.relatedTarget) && !document.getElementById(detailId)?.contains(event.relatedTarget)) { setFocused(false); setDismissed(false); } }}
    onKeyDown={(event) => { if (event.key === "Escape") { close(); } else { touch.current = false; setFocused(true); } }}>
    <div className="online-result-summary">
      {multi ? <input type="checkbox" aria-label={`选择 ${title}`} checked={checked} disabled={item.availability?.download_disabled} onChange={onToggle} /> : null}
      <button className="online-result-main" data-result-index={index} aria-label={`查看 ${title}`} aria-current={selected ? "true" : undefined} onClick={onChoose} onKeyDown={onNavigate}>
        <span className="online-result-cover">{cover ? <img src={cover} alt="" loading="lazy" onError={(event) => { event.currentTarget.style.visibility = "hidden"; }} /> : null}</span>
        <span className="online-result-copy"><strong title={title}>{title}</strong><span title={displayArtist(item)}>{displayArtist(item)}</span><small title={`谱师 ${item.creator}`}>谱师 {item.creator}</small></span>
      </button>
    </div>
    <div className="online-result-meta"><OnlineStatusBadge status={item.status} /><span>{starRange(item.beatmaps)}</span>{queued ? <small>已在清单</small> : null}{item.availability?.download_disabled ? <small>禁止下载</small> : null}</div>
    <div className="online-result-bottom">
      <button className="online-result-difficulty-toggle" aria-label={`${difficulties.length} 个难度`} title="按向下方向键进入难度详情" aria-expanded={open} aria-controls={detailId} onKeyDown={(event) => {
        if (event.key === "ArrowDown") {
          event.preventDefault(); event.stopPropagation(); setFocused(true); setDismissed(false);
          requestAnimationFrame(() => document.getElementById(detailId)?.focus({ preventScroll: true }));
        }
      }} onClick={() => { setTouchOpen(!open); setDismissed(open); }}>
        <span className="online-result-difficulty-icons">{difficulties.slice(0, 6).map((difficulty) => <DifficultyIcon key={difficulty.id} mode={difficulty.mode} stars={difficulty.difficulty_rating} showValue={false} className="online-result-difficulty-icon" />)}{difficulties.length > 6 ? <i>+{difficulties.length - 6}</i> : null}</span><ChevronDown className={open ? "is-open" : ""} />
      </button>
      <div className="online-result-actions"><button aria-label={`${playing ? "暂停试听" : "试听"} ${title}`} title={playing ? "暂停试听" : "试听"} disabled={!normalizePreviewUrl(item.preview_url)} onClick={onPreview}>{playing ? <Pause /> : <Headphones />}</button><button aria-label={`下载 ${title}`} title="下载" disabled={busy || item.availability?.download_disabled} onClick={onDownload}><Download /></button></div>
    </div>
    {open && present ? <OnlineDifficultyPopover anchor={root} item={item} id={detailId} onEnter={enter} onLeave={leave} onClose={close} /> : null}
  </div>;
}
