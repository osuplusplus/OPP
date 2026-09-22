import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Folder, FolderOpen } from "lucide-react";
import type { CollectionFolderSummary } from "../../shared/types/osu";
import { useCollectionArtwork, useCollectionBrowser } from "./api";
import { CollectionThumbnail } from "./CollectionThumbnail";
import { collectionDuration, collectionMetric } from "./browserModel";

function FolderPreview({ folder, anchor, id, onEnter, onLeave }: { folder: CollectionFolderSummary; anchor: HTMLButtonElement | null; id: string; onEnter: () => void; onLeave: () => void }) {
  const panel = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState({ left: 0, top: 0 });
  const artwork = useCollectionArtwork(folder.id);
  const details = useCollectionBrowser({ folder_id: folder.id, search: "", sort: "auto", offset: 0, limit: 5, player: null }, true);
  const overview = details.data?.overview;
  useLayoutEffect(() => {
    const place = () => {
      if (!anchor || !panel.current) return;
      const box = anchor.getBoundingClientRect();
      const { width, height } = panel.current.getBoundingClientRect();
      const beside = box.right + width + 24 <= window.innerWidth ? box.right + 12 : box.left - width - 12;
      setPosition({ left: Math.max(12, Math.min(beside < 12 ? box.left : beside, window.innerWidth - width - 12)), top: Math.max(12, Math.min(box.top, window.innerHeight - height - 12)) });
    };
    place();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(place);
    if (panel.current) observer?.observe(panel.current);
    window.addEventListener("resize", place); window.addEventListener("scroll", place, true);
    return () => { observer?.disconnect(); window.removeEventListener("resize", place); window.removeEventListener("scroll", place, true); };
  }, [anchor]);
  const range = (values: [number, number] | null | undefined, digits: number) => values ? `${collectionMetric(values[0], digits)}–${collectionMetric(values[1], digits)}` : "—";
  return createPortal(<div id={id} ref={panel} className="collection-folder-preview" role="tooltip" style={position} onMouseEnter={onEnter} onMouseLeave={onLeave}>
    <div className="collection-preview-heading"><span>{folder.source.toUpperCase()} · {folder.creator || "未署名"}</span><strong>{folder.name}</strong></div>
    <div className="collection-folder-covers">{artwork.data?.slice(0, 4).map((item) => <CollectionThumbnail item={item} key={`${item.client}:${item.resource_id}`} />)}</div>
    <div className="collection-preview-stats"><span><b>{folder.entry_count}</b>难度</span><span><b>{folder.beatmapset_count}</b>曲包</span><span><b>{overview?.local_count ?? "—"}</b>本地可用</span><span><b>{folder.missing_count}</b>待补齐</span></div>
    {overview && <>
      <div className="collection-preview-ranges"><span>NM 星数 <b>{range(overview.stars, 2)} ★</b></span><span>BPM <b>{range(overview.bpm, 0)}</b></span><span>本地总时长 <b>{collectionDuration(overview.total_length_ms)}</b></span></div>
      {!!overview.slots.length && <div className="collection-preview-slots">{overview.slots.map(([name, count]) => <span key={name}>{name} <b>{count}</b></span>)}</div>}
      <p className="collection-preview-progress">已标记 {overview.marked_count} / {folder.entry_count} · 已写笔记 {overview.noted_count} / {folder.entry_count}</p>
    </>}
    <div className="collection-preview-tracklist"><small>谱面速览{details.data && details.data.total > 5 ? ` · 前 5 / ${details.data.total} 项` : ""}</small>
      {details.isPending ? <p>正在读取曲包信息…</p> : details.isError ? <p>详细信息暂时不可用，进入收藏夹后重试。</p> : details.data?.items.map((row) => <div key={row.key}><b>{row.slot || "—"}</b><span title={`${row.entry.title} [${row.entry.difficulty_name}]`}>{row.entry.title}<small>{row.entry.difficulty_name}{row.selected_by ? ` · 选图 ${row.selected_by}` : ""}</small></span><em>{row.local?.stars == null ? "未安装" : `${row.local.stars.toFixed(2)} ★`}</em></div>)}
    </div>
    <p className="collection-preview-footer">{folder.read_only ? "游戏收藏只读 · 可记录个人笔记" : folder.pending_write ? "成员修改待写回游戏" : "OPP 收藏已保存"} · 单击进入</p>
  </div>, document.body);
}

export function CollectionFolderTile({ folder, onOpen }: { folder: CollectionFolderSummary; onOpen: () => void }) {
  const [preview, setPreview] = useState(false);
  const [anchor, setAnchor] = useState<HTMLButtonElement | null>(null);
  const id = useId();
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const start = () => { clearTimeout(timer.current); timer.current = setTimeout(() => setPreview(true), 450); };
  const stop = () => { clearTimeout(timer.current); setPreview(false); };
  const leave = () => { clearTimeout(timer.current); timer.current = setTimeout(() => setPreview(false), 120); };
  useEffect(() => () => clearTimeout(timer.current), []);
  return <><button ref={setAnchor} type="button" className={`collection-folder-tile ${preview ? "is-previewing" : ""}`} onClick={() => { stop(); onOpen(); }} onMouseEnter={start} onMouseLeave={leave} onFocus={start} onBlur={stop} onKeyDown={(event) => { if (event.key === "Escape") stop(); }} aria-label={`打开收藏夹 ${folder.name}`} aria-describedby={preview ? id : undefined}>
    <span className="collection-folder-icon">{preview ? <FolderOpen /> : <Folder />}</span>
    <strong title={folder.name}>{folder.name}</strong><span>{folder.entry_count} 个难度 · {folder.beatmapset_count} 个曲包</span>
    <small>{folder.source.toUpperCase()}{folder.pending_write ? " · 待写回" : ""}{folder.read_only ? " · 只读" : ""}{folder.missing_count > 0 ? ` · 缺失 ${folder.missing_count}` : " · 已齐全"}</small>
  </button>{preview && <FolderPreview folder={folder} anchor={anchor} id={id} onEnter={() => clearTimeout(timer.current)} onLeave={leave} />}</>;
}
