import { useLayoutEffect, useRef, useState, type CSSProperties, type RefObject } from "react";
import { createPortal } from "react-dom";
import type { OnlineBeatmapset } from "../../shared/types/osu";
import { DifficultyIcon } from "../../shared/components/DifficultyIcon";
import { fixedNumber } from "../../shared/lib/format";
import { displayTitle } from "./stageModel";
import { useQuietScrollbar } from "./useQuietScrollbar";
import { LocalDifficultyBadge } from "./localPresence";
import type { PresenceMap } from "./localPresenceModel";
import { scrollWheel } from "./scrollWheel";

export function OnlineDifficultyPopover({ presence, anchor, item, id, onClose, onChoose }: {
  presence?: PresenceMap; anchor: RefObject<HTMLDivElement | null>; item: OnlineBeatmapset; id: string;
  onClose: () => void; onChoose: (beatmapId: number) => void;
}) {
  const [style, setStyle] = useState<CSSProperties>({ visibility: "hidden" });
  const onScroll = useQuietScrollbar();
  const popupRef = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const node = anchor.current;
    if (!node) return;
    const stage = node.closest(".online-stage");
    const popup = popupRef.current;
    // Keep wheel input inside the difficulty list. Passing the result list as an
    // owner causes a partially consumed wheel delta to scroll both surfaces.
    const wheel = (event: WheelEvent) => { if (popup) { event.stopPropagation(); scrollWheel(event, popup, null); } };
    popup?.addEventListener("wheel", wheel, { passive: false });
    const position = () => {
      const rect = node.getBoundingClientRect();
      const scroll = node.closest(".online-results-scroll, .online-home-scroll")?.getBoundingClientRect();
      const trend = node.closest(".online-trend-list")?.getBoundingClientRect();
      if (node.closest("[inert]") || (scroll && scroll.height > 0 && (rect.bottom <= scroll.top || rect.top >= scroll.bottom)) || (trend && trend.width > 0 && (rect.right <= trend.left || rect.left >= trend.right))) { onClose(); return; }
      const below = window.innerHeight - rect.bottom - 14;
      const above = rect.top - 14;
      const upward = below < 120 && above > below;
      const width = Math.min(trend ? Math.max(280, rect.width) : rect.width, window.innerWidth - 16);
      const next: CSSProperties & Record<string, string | number | undefined> = {
        width, left: Math.max(8, Math.min(rect.left, window.innerWidth - width - 8)),
        maxHeight: Math.max(40, Math.min(240, upward ? above : below)),
        ...(upward ? { bottom: window.innerHeight - rect.top + 6 } : { top: rect.bottom + 6 }),
      };
      if (stage) {
        const computed = getComputedStyle(stage);
        for (const key of ["--online-ink", "--online-muted", "--online-line", "--online-panel", "--stage-accent"]) next[key] = computed.getPropertyValue(key);
      }
      setStyle((current) => JSON.stringify(current) === JSON.stringify(next) ? current : next);
    };
    position();
    const observer = new ResizeObserver(position); observer.observe(node);
    const mutations = new MutationObserver(position);
    if (stage) mutations.observe(stage, { attributes: true, attributeFilter: ["style", "class", "inert"] });
    const dismiss = (event: KeyboardEvent) => { if (event.key === "Escape") onClose(); };
    window.addEventListener("resize", position);
    document.addEventListener("scroll", position, true);
    document.addEventListener("keydown", dismiss);
    return () => { popup?.removeEventListener("wheel", wheel); observer.disconnect(); mutations.disconnect(); window.removeEventListener("resize", position); document.removeEventListener("scroll", position, true); document.removeEventListener("keydown", dismiss); };
  }, [anchor, onClose]);
  const difficulties = [...(item.beatmaps ?? [])].sort((a, b) => a.difficulty_rating - b.difficulty_rating || a.id - b.id);
  return createPortal(<div ref={popupRef} id={id} className="online-result-difficulty-detail online-difficulty-popover online-quiet-scroll" style={style} tabIndex={0} role="region" aria-label={`${displayTitle(item)} 的难度详情`} onScroll={onScroll}>
    {difficulties.length ? difficulties.map((difficulty) => <button key={difficulty.id} type="button" onClick={() => onChoose(difficulty.id)}>
      <DifficultyIcon mode={difficulty.mode} stars={difficulty.difficulty_rating} showValue={false} className="online-result-difficulty-icon" />
      <strong title={difficulty.version}>{difficulty.version}</strong><small><LocalDifficultyBadge id={difficulty.id} presence={presence} /> {fixedNumber(difficulty.difficulty_rating, 2)}★ · BPM {fixedNumber(difficulty.bpm ?? item.bpm)} · AR {fixedNumber(difficulty.ar, 1)} · OD {fixedNumber(difficulty.accuracy, 1)}</small>
    </button>) : <small>暂无详细难度数据</small>}
  </div>, document.body);
}
