import { useEffect, useId, useRef, useState } from "react";
import { Download, Heart, Headphones, Pause, ListMusic, ImageIcon, ExternalLink, ScanSearch, Search, Ellipsis } from "lucide-react";
import type { OnlineBeatmapset, Ruleset } from "../../shared/types/osu";
import { durationLabel, normalizePreviewUrl } from "./filters";
import { displayArtist, displayTitle } from "./stageModel";
import { useQuietScrollbar } from "./useQuietScrollbar";
import { dateOnly } from "../../shared/lib/format";
import { LocalSetBadge, LocalDifficultyBadge } from "./localPresence";
import { useOnlineLocalPresence } from "./useOnlineLocalPresence";
import { OnlineStatusBadge } from "./OnlineStatusBadge";

export function OnlineStageSong({ set, ruleset, playing, busy, loading, onPreview, onDownload, onDetails, onCollect, onVisualPreview, onSimilar, onWebsite, onDifficultyWebsite, onSearchTitle, initialBeatmapId }: {
  set: OnlineBeatmapset; ruleset: Ruleset | null; playing: boolean; busy: boolean; loading: boolean;
  onPreview: () => void; onDownload: () => void; onDetails: () => void; onCollect: () => void; onVisualPreview: () => void; onSimilar: (id: number, ruleset: Ruleset) => void; onWebsite: () => void;
  initialBeatmapId?: number | null;
  onDifficultyWebsite: (id: number, mode: Ruleset) => void; onSearchTitle: () => void;
}) {
  const presence = useOnlineLocalPresence([set]);
  const [difficultyId, setDifficultyId] = useState<number | null>(initialBeatmapId ?? null);
  const onDifficultyScroll = useQuietScrollbar();
  const [moreOpen, setMoreOpen] = useState(false);
  const more = useRef<HTMLDivElement>(null);
  const moreButton = useRef<HTMLButtonElement>(null);
  const moreId = useId();
  useEffect(() => {
    if (!moreOpen) return;
    const close = (event: PointerEvent) => { if (!more.current?.contains(event.target as Node)) setMoreOpen(false); };
    document.addEventListener("pointerdown", close);
    return () => document.removeEventListener("pointerdown", close);
  }, [moreOpen]);
  const difficulties = [...(set.beatmaps ?? [])].sort((a, b) => a.difficulty_rating - b.difficulty_rating || a.id - b.id);
  const difficulty = difficulties.find((item) => item.id === difficultyId) ?? difficulties.find((item) => !ruleset || item.mode === ruleset) ?? difficulties[0];
  const number = (value?: number) => value == null || !Number.isFinite(value) ? "—" : Number(value.toFixed(2)).toString();
  return <div className="online-stage-song">
    <div className="online-song-heading"><div className="online-song-meta"><OnlineStatusBadge status={set.status} /><LocalSetBadge set={set} presence={presence} /><span>BID {difficulty?.id ?? "—"}</span><button aria-label="打开当前难度官网" title="打开当前难度官网" disabled={!difficulty} onClick={() => { if (difficulty) onDifficultyWebsite(difficulty.id, difficulty.mode); }}><ExternalLink /></button>{loading ? <span>正在读取详情…</span> : null}</div>
      <p>{displayArtist(set)}</p><div className="online-song-title"><h1 title={displayTitle(set)}>{displayTitle(set)}</h1><button aria-label="搜索同名" title="搜索同名" onClick={onSearchTitle}><Search /></button></div>
      <div className="online-song-credits"><small>谱师 <strong>{set.creator}</strong></small><small>上架 {dateOnly(set.ranked_date)}</small><small>上传 {dateOnly(set.submitted_date)}</small></div>
    </div>
    <div className="online-difficulties online-quiet-scroll" onScroll={onDifficultyScroll} role="tablist" aria-label="谱面难度">
      {difficulties.map((item, index) => <button key={item.id} role="tab" aria-selected={item.id === difficulty?.id} tabIndex={item.id === difficulty?.id ? 0 : -1} title={`${item.mode} · ${item.version}`} onClick={() => setDifficultyId(item.id)} onKeyDown={(event) => {
        let next: number;
        if (event.key === "ArrowRight") next = (index + 1) % difficulties.length;
        else if (event.key === "ArrowLeft") next = (index - 1 + difficulties.length) % difficulties.length;
        else if (event.key === "Home") next = 0;
        else if (event.key === "End") next = difficulties.length - 1;
        else return;
        event.preventDefault(); setDifficultyId(difficulties[next].id);
        const target = event.currentTarget.parentElement?.children[next] as HTMLElement | undefined;
        target?.focus({ preventScroll: true }); target?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
      }}><span>{item.version}</span><LocalDifficultyBadge id={item.id} presence={presence} /><small>{number(item.difficulty_rating)} ★</small></button>)}
    </div>
    {difficulty ? <div className="online-song-metrics">{[
      ["stars", "星级", `${number(difficulty.difficulty_rating)} ★`],
      ["bpm", "BPM", number(difficulty.bpm ?? set.bpm)],
      ["length", "时长", durationLabel(difficulty.total_length)],
      ["ar", "AR", number(difficulty.ar)],
      ["od", "OD", number(difficulty.accuracy)],
      ["cs", "CS", number(difficulty.cs)],
      ["hp", "HP", number(difficulty.drain)],
    ].map(([key, label, value]) => <div key={key} data-metric={key}><span>{label}</span><strong>{value}</strong></div>)}</div> : <p className="online-notice">暂无难度数据，可打开完整详情重试。</p>}
    <div className="online-song-actions">
      <button aria-label={playing ? "暂停试听" : "试听"} disabled={!normalizePreviewUrl(set.preview_url)} onClick={onPreview}>{playing ? <Pause /> : <Headphones />}{playing ? "暂停试听" : "试听"}</button>
      <button className="is-primary" disabled={busy || set.availability?.download_disabled} title={set.availability?.download_disabled ? "该谱面禁止下载" : "下载谱面集"} onClick={onDownload}><Download />下载</button>
      <button onClick={onCollect}><Heart />收藏</button>
      <div className="online-song-more" ref={more} onBlur={(event) => { if (!event.currentTarget.contains(event.relatedTarget)) setMoreOpen(false); }} onKeyDown={(event) => { if (event.key === "Escape") { setMoreOpen(false); moreButton.current?.focus(); } }}>
        <button ref={moreButton} aria-label="更多功能" title="更多功能" aria-expanded={moreOpen} aria-controls={moreId} onClick={() => setMoreOpen(!moreOpen)}><Ellipsis /></button>
        {moreOpen ? <div id={moreId} className="online-song-more-panel" role="group" aria-label="更多谱面功能" onClick={(event) => { if ((event.target as HTMLElement).closest("button:not(:disabled)")) { setMoreOpen(false); moreButton.current?.focus(); } }}>
          <button onClick={onDetails}><ListMusic />完整详情</button>
          <button disabled={!difficulty} onClick={() => { if (difficulty) onSimilar(difficulty.id, difficulty.mode); }}><ScanSearch />查找相似</button>
          <button disabled={!difficulty} onClick={onVisualPreview}><ImageIcon />预览</button><button onClick={onWebsite}><ExternalLink />官网</button>
        </div> : null}
      </div>
    </div>
  </div>;
}
