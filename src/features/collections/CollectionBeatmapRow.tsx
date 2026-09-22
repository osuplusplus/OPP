import { useEffect, useRef } from "react";
import { MoreHorizontal, Music2, StickyNote, Tags, Trophy } from "lucide-react";
import type { CollectionBrowseRow } from "../../shared/types/osu";
import { desktopApi } from "../../shared/lib/tauri";
import { errorMessage } from "../../shared/lib/format";
import { openCollectionDialog } from "./events";
import { similarityRouteForLocalResource } from "../similar-beatmaps/navigation";
import { beatmapPreviewRoute } from "../tools/navigation";
import { musicApi } from "../music-player/api";
import { CollectionThumbnail } from "./CollectionThumbnail";
import { collectionDuration, collectionMetric, collectionScoreSource, collectionSlotGroup, highlightParts, scoreLabel } from "./browserModel";

export function CollectionBeatmapRow({ row, search, onOpen, onRecord, onDetail, onNavigate, onChanged, onDownload, onNotice }: {
  row: CollectionBrowseRow; search: string; onOpen: () => void; onRecord: () => void;
  onDetail: () => void; onNavigate: (path: string) => void; onChanged: (folderId: string, entryId?: string) => Promise<void>;
  onDownload: (folderId: string) => Promise<void>; onNotice: (notice: string) => void;
}) {
  const menu = useRef<HTMLDetailsElement>(null);
  useEffect(() => {
    const close = (event: PointerEvent) => { if (menu.current && !menu.current.contains(event.target as Node)) menu.current.open = false; };
    document.addEventListener("pointerdown", close);
    return () => document.removeEventListener("pointerdown", close);
  }, []);
  const local = row.local;
  const run = async (action: () => Promise<unknown> | void) => {
    if (menu.current) menu.current.open = false;
    try { await action(); } catch (error) { onNotice(errorMessage(error)); }
  };
  const preview = beatmapPreviewRoute(row.entry.beatmap_id);
  const score = row.record.representative ?? row.latest_score;
  return <article className="collection-beatmap-row" data-row-key={row.key}>
    <span className="collection-slot" data-group={collectionSlotGroup(row.slot)}>{row.slot || "—"}<small>{local?.ruleset === "mania" ? `${collectionMetric(local.cs, 0)}K` : (local?.ruleset ?? row.entry.ruleset ?? "osu").replace("fruits", "catch")}</small></span>
    <button className="collection-map-main" onClick={onOpen} aria-label={`打开谱面 ${row.entry.title} ${row.entry.difficulty_name}`}>
      <CollectionThumbnail item={local?.resource ?? null} /><span><strong title={row.entry.title}>{row.entry.title || `谱面 #${row.entry.beatmap_id ?? "未知"}`}</strong><small title={`${row.entry.artist} · ${row.entry.difficulty_name}`}>{row.entry.artist} · [{row.entry.difficulty_name || "未解析难度"}]</small><small title={row.entry.creator}>{row.entry.creator || "未知谱师"} <span className="collection-map-id">#{row.entry.beatmap_id ?? "本地"}</span>{search && ` · ${row.folder_name}`}</small></span>
    </button>
    <div className="collection-map-metrics" aria-label="谱面 NM 参数">
      <div className="collection-metric-primary"><strong>{local?.stars == null ? "—" : `${local.stars.toFixed(2)} ★`}</strong><span>{collectionMetric(local?.bpm, 0)} <small>BPM</small></span><span>{collectionDuration(local?.length_ms)}</span><small className="collection-base-mods" title="本地索引中的 NM 参数，未应用图位 Mods">NM</small></div>
      <div className="collection-difficulty-stats">{(["ar", "od", "cs", "hp"] as const).map((key) => <span key={key} data-metric={key}><small>{key === "cs" && local?.ruleset === "mania" ? "KEY" : key.toUpperCase()}</small><b>{collectionMetric(local?.[key])}</b></span>)}</div>
      <small className="collection-density">{local ? <>峰值 {collectionMetric(local.peak_nps)} NPS <span>·</span> {local.object_count.toLocaleString()} 物件 <span>·</span> FC {local.max_combo?.toLocaleString() ?? "—"}x</> : "本地待补齐 · 参数暂不可用"}</small>
    </div>
    <button className="collection-row-record" aria-label={`编辑 ${row.entry.title} 的标签和笔记`} onClick={onRecord}>
      <span className="collection-tags">{row.record.tags.length ? row.record.tags.slice(0, 3).map((tag, i) => <span style={{ color: tag.color, borderColor: tag.color }} key={i}>{tag.name}</span>) : <span className="collection-empty-tag"><Tags size={12} />添加标记</span>}{row.record.tags.length > 3 && <span>+{row.record.tags.length - 3}</span>}{row.selected_by && <span className="collection-picker" title={`选图人：${row.selected_by}`}>选图 · {row.selected_by}</span>}</span>
      {row.pool_comment && <small className="collection-pool-comment" title={row.pool_comment}><span>比赛</span><span>{row.pool_comment}</span></small>}
      <small title={row.record.note || undefined}><StickyNote size={12} /><span>{row.record.note || "记录打法、选图意向或练习目标…"}</span></small>
    </button>
    <button className="collection-row-score" onClick={onRecord} aria-label={`查看 ${row.entry.title} 的成绩`}>
      <small><Trophy size={12} />{row.record.representative ? "代表成绩" : "最近"}{score && <span title={score.mods || "NM"}>{score.mods || "NM"}</span>}</small><strong>{scoreLabel(score)}</strong>
      {score ? <><small>{score.combo == null ? "Combo —" : `${score.combo.toLocaleString()}x`} · {collectionScoreSource(score)}{row.record.representative && !row.representative_available ? " · 快照" : ""}</small><small title={score.player}>{score.player || "手动记录"}{score.played_at ? ` · ${new Date(score.played_at).toLocaleDateString()}` : ""}</small></> : <small>记录一次练习 →</small>}
    </button>
    <details className="collection-row-menu" ref={menu} onKeyDown={(e) => { if (e.key === "Escape" && menu.current) { menu.current.open = false; menu.current.querySelector("summary")?.focus(); } }}><summary aria-label={`${row.entry.title} 更多操作`}><MoreHorizontal size={20} /></summary><div>
      <button disabled={!local} onClick={() => void run(onDetail)}>完整详情</button>
      <button disabled={!local} onClick={() => void run(() => musicApi.setQueue({ client: local!.resource.client, resource_id: local!.resource.resource_id, append: true, preview: true }))}><Music2 size={14} />试听</button>
      <button disabled={!local} onClick={() => void run(() => onNavigate(similarityRouteForLocalResource(local!.resource.client, local!.resource.resource_id, local!.ruleset)))}>查找相似</button>
      <button onClick={() => void run(() => openCollectionDialog([{ ...row.entry, local_client: local?.resource.client ?? null, local_resource_id: local?.resource.resource_id ?? null }]))}>加入其他收藏夹</button>
      <button disabled={!local} onClick={() => void run(() => onNavigate(`/view-trainer?${new URLSearchParams({ client: local!.resource.client, resource: local!.resource.resource_id })}`))}>导入 Trainer</button>
      <button disabled={!preview} onClick={() => void run(() => { if (preview) onNavigate(preview); })}>生成预览</button>
      <button disabled={!local} onClick={() => void run(async () => {
        if (!local) return;
        if (local.resource.client === "stable" && local.resource.logical_path) await desktopApi.openLocalResourceInExplorer("stable", local.resource.logical_path);
        else { const directory = await desktopApi.chooseDirectory("选择 .osz 导出位置"); if (directory) onNotice(`已导出：${await desktopApi.exportLocalBeatmapSet(local.resource.client, local.set_key, directory)}`); }
      })}>{local?.resource.client === "lazer" ? "导出 .osz" : "打开本地文件"}</button>
      <button disabled={!row.entry.beatmap_id} onClick={() => void run(() => desktopApi.openExternal(`https://osu.ppy.sh/beatmaps/${row.entry.beatmap_id}`))}>谱面官网</button>
      {!local && <button disabled={row.read_only} onClick={() => void run(() => onDownload(row.folder_id))}>补齐收藏夹谱面</button>}
      <button onClick={() => void run(onRecord)}>编辑个人记录</button>
      <button className="is-danger" disabled={row.read_only} onClick={() => void run(() => onChanged(row.folder_id, row.entry.id))}>移出收藏夹</button>
    </div></details>
    {search && row.matches.length > 0 && <div className="collection-search-hits">{row.matches.map((hit, i) => <p key={i}><small>{hit.field}</small>{highlightParts(hit.text, search).map((part, j) => part.matched ? <mark key={j}>{part.text}</mark> : part.text)}</p>)}</div>}
  </article>;
}
