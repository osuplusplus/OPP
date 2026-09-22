import { type ReactNode, useEffect, useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useSearchParams } from "react-router-dom";
import { ArrowLeft, ChevronRight, FolderOpen, Grid2X2, Search, X } from "lucide-react";
import { desktopApi } from "../../shared/lib/tauri";
import { errorMessage } from "../../shared/lib/format";
import type { CollectionBrowseRow, CollectionFolderSummary } from "../../shared/types/osu";
import { BeatmapDetailDrawer } from "../local-analysis/LocalDetailDrawers";
import { useLocalIndexStatus } from "../local-analysis/api";
import { collectionBrowserKey, useCollectionBrowser } from "./api";
import { CollectionFolderTile } from "./CollectionFolderTile";
import { CollectionBeatmapRow } from "./CollectionBeatmapRow";
import { CollectionRecordDrawer } from "./CollectionRecordDrawer";
import { CollectionBackdrop } from "./CollectionBackdrop";
import { localCollectionRoute, readSession, saveSession } from "./browserModel";
import "./collections.css";

export function CollectionExplorer({ folders, loading, failed, toolbar, onRetry, onChanged, onDownload, onNotice }: {
  folders: CollectionFolderSummary[]; loading: boolean; failed: boolean; toolbar: ReactNode;
  onRetry: () => void; onChanged: (folderId?: string, entryId?: string) => Promise<void>;
  onDownload: (folderId: string) => Promise<void>; onNotice: (message: string) => void;
}) {
  const [params, setParams] = useSearchParams();
  useLocalIndexStatus();
  const navigate = useNavigate(); const client = useQueryClient();
  const folderId = params.get("folder"); const folder = folders.find((f) => f.id === folderId);
  const search = params.get("q") ?? "";
  const [debounced, setDebounced] = useState(search);
  const [playerChoice, setPlayerChoice] = useState<string | null>(() => readSession("opp:collection-player", null));
  const [animated, setAnimated] = useState(() => readSession("opp:collection-motion", true));
  const [record, setRecord] = useState<CollectionBrowseRow | null>(null);
  const [detail, setDetail] = useState<CollectionBrowseRow | null>(null);
  const [rename, setRename] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [busy, setBusy] = useState(false);
  const status = useQuery({ queryKey: ["local-score-status"], queryFn: () => desktopApi.refreshLocalScores(), staleTime: 0, retry: false, refetchOnWindowFocus: true });
  const player = playerChoice ?? status.data?.default_player ?? null;
  const pageParam = search.trim() ? "resultsPage" : "page";
  const page = Math.max(0, Number(params.get(pageParam)) || 0);
  const sort = params.get("sort") ?? "auto";
  const query = useCollectionBrowser({ folder_id: folderId, search: debounced, sort, offset: page * 50, limit: 50, player }, !!folderId || !!debounced.trim());
  const positionKey = `opp:collection-position:${params.toString()}`;
  const restored = useRef<string | null>(null);
  const activeRow = useRef<string | null>(null);
  useEffect(() => { const timer = setTimeout(() => setDebounced(search), 200); return () => clearTimeout(timer); }, [search]);
  useEffect(() => {
    if (!status.dataUpdatedAt) return;
    void client.invalidateQueries({ queryKey: collectionBrowserKey });
    void client.invalidateQueries({ queryKey: ["collection-scores"] });
  }, [client, status.dataUpdatedAt]);
  useEffect(() => {
    let active = true; let dispose: (() => void) | undefined;
    void desktopApi.onGameStatusChanged(() => { void client.invalidateQueries({ queryKey: ["local-score-status"] }); }).then((fn) => { if (active) dispose = fn; else fn(); }).catch(() => undefined);
    return () => { active = false; dispose?.(); };
  }, [client]);
  useEffect(() => {
    const save = () => { if (restored.current === positionKey) saveSession(positionKey, { top: window.scrollY, row: activeRow.current }); };
    window.addEventListener("scroll", save, { passive: true });
    return () => { save(); window.removeEventListener("scroll", save); };
  }, [positionKey]);
  useEffect(() => {
    if (loading || query.isFetching || search !== debounced || restored.current === positionKey) return;
    const frame = requestAnimationFrame(() => {
      const previous = readSession<{ top: number; row: string | null }>(positionKey, { top: 0, row: null });
      activeRow.current = previous.row;
      if (previous.row) document.querySelector<HTMLElement>(`[data-row-key="${CSS.escape(previous.row)}"] button`)?.focus({ preventScroll: true });
      window.scrollTo({ top: previous.top, behavior: "instant" }); restored.current = positionKey;
    });
    return () => cancelAnimationFrame(frame);
  }, [loading, query.isFetching, positionKey, search, debounced]);
  const update = (mutate: (next: URLSearchParams) => void, replace = false) => { const next = new URLSearchParams(params); mutate(next); setParams(next, { replace }); };
  const openFolder = (id: string | null) => update((next) => { if (id) next.set("folder", id); else next.delete("folder"); next.delete("q"); next.delete("page"); next.delete("resultsPage"); });
  const go = (path: string, row: CollectionBrowseRow) => {
    activeRow.current = row.key; saveSession(positionKey, { top: window.scrollY, row: row.key });
    navigate(path, { state: { collectionReturn: `/collections?${params}` } });
  };
  const run = async (action: () => Promise<unknown>) => { setBusy(true); try { await action(); } catch (error) { onNotice(errorMessage(error)); } finally { setBusy(false); } };
  const refreshScores = () => run(async () => { const data = await desktopApi.refreshLocalScores(true); client.setQueryData(["local-score-status"], data); });
  const shownFolders = search.trim() ? folders.filter((f) => query.data?.matching_folders.includes(f.id)) : folders;
  const inResults = !!search.trim();
  return <section className="collection-explorer">
    <CollectionBackdrop key={folderId ?? "all"} folderId={folderId} animated={animated} />
    <header className="collection-header"><div><small>YOUR BEATMAP LIBRARY</small><h1>谱面收藏夹</h1><p>整理曲包，记录每一次练习。</p></div><label className="collection-motion-toggle"><input type="checkbox" checked={animated} onChange={(e) => { setAnimated(e.target.checked); saveSession("opp:collection-motion", e.target.checked); }} />动态背景</label></header>
    <div className="collection-toolbar">{toolbar}</div>
    <div className="collection-search"><Search size={20} /><input aria-label="搜索全部收藏" placeholder="搜索全部收藏 · 曲名、图位、标签、评论中的只言片语…" value={search} onChange={(e) => update((next) => { if (e.target.value) next.set("q", e.target.value); else next.delete("q"); next.delete("resultsPage"); }, true)} />{search && <button aria-label="清空搜索" onClick={() => update((next) => { next.delete("q"); next.delete("resultsPage"); }, true)}><X size={18} /></button>}<span>全部收藏</span></div>
    <div className="collection-browser-toolbar"><nav aria-label="收藏夹路径"><button onClick={() => openFolder(null)}><Grid2X2 size={16} />全部收藏</button>{folderId && <><ChevronRight size={15} /><button onClick={() => openFolder(folderId)}>{folder?.name ?? "收藏夹"}</button></>}{inResults && <><ChevronRight size={15} /><span>搜索结果</span></>}</nav><div>
      <select aria-label="成绩玩家" value={player ?? ""} onChange={(e) => { setPlayerChoice(e.target.value); saveSession("opp:collection-player", e.target.value); }}><option value="">选择成绩玩家</option>{status.data?.players.map((name) => <option key={name} value={name}>{name || "未命名玩家"}</option>)}</select>
      <button disabled={status.isFetching || busy} onClick={() => void refreshScores()}>{status.isFetching ? "读取成绩…" : "刷新成绩"}</button>
      {(folderId || inResults) && <select aria-label="谱面排序" value={sort} onChange={(e) => update((next) => { next.set("sort", e.target.value); next.delete("page"); next.delete("resultsPage"); }, true)}><option value="auto">图位 / 收藏顺序</option><option value="order">收藏顺序</option><option value="title">曲名</option><option value="stars">星数</option></select>}
    </div></div>
    {status.data?.errors.map((error) => <p className="collection-inline-notice" key={error}>{error}；已保留上次读取结果。</p>)}
    {status.isError && <p className="collection-inline-notice">本地成绩读取失败，可点击刷新成绩重试。</p>}
    {folder && !inResults && <div className="collection-folder-heading"><div><button aria-label="返回全部收藏" onClick={() => openFolder(null)}><ArrowLeft size={18} /></button><FolderOpen size={25} /><h2>{folder.name}</h2><small>{folder.entry_count} 个难度</small></div><div>
      {query.data?.pool && <button disabled={busy} onClick={() => void run(async () => { await desktopApi.syncTournamentPoolCollection(query.data!.pool!.reference); await onChanged(); })}>同步比赛图池</button>}
      <button disabled={busy} onClick={() => void run(async () => { const code = await desktopApi.exportCollectionShare(folder.id, folder.creator); await navigator.clipboard.writeText(code); onNotice("分享码已复制"); })}>导出分享码</button>
      <button disabled={busy || folder.read_only} onClick={() => setRename(folder.name)}>重命名</button>
      <button disabled={busy || folder.read_only} onClick={() => void run(() => onDownload(folder.id))}>补齐谱面</button>
      <button disabled={busy || folder.read_only} className="is-danger" onClick={() => setDeleting(true)}>删除</button>
    </div></div>}
    {loading ? <p className="collection-empty" role="status">正在读取收藏夹…</p> : failed ? <button onClick={onRetry}>读取收藏夹失败，点击重试</button> : !folderId && !inResults && !folders.length ? <div className="collection-empty"><FolderOpen size={42} /><h2>还没有收藏夹</h2><p>新建收藏夹，或从游戏和分享码导入你的曲包。</p></div> : null}
    {(!folderId || inResults) && shownFolders.length > 0 && <div className="collection-folder-grid">{shownFolders.map((f) => <CollectionFolderTile folder={f} key={f.id} onOpen={() => openFolder(f.id)} />)}</div>}
    {(folderId || inResults) && <>
      {query.isError ? <button onClick={() => void query.refetch()}>读取谱面失败，点击重试</button> : query.isLoading || search !== debounced ? <p className="collection-empty" role="status">正在查找谱面…</p> : query.data?.items.length ? <div className="collection-rows">{query.data.items.map((row) => <CollectionBeatmapRow key={row.key} row={row} search={search} onOpen={() => { const path = localCollectionRoute(row); if (path) go(path, row); else { onNotice("该难度尚未在本地找到，可通过更多操作补齐谱面。"); setRecord(row); } }} onRecord={() => setRecord(row)} onDetail={() => setDetail(row)} onNavigate={(path) => go(path, row)} onChanged={onChanged} onDownload={onDownload} onNotice={onNotice} />)}</div> : <p className="collection-empty">{inResults ? "没有匹配内容，试试更短的词句。" : "收藏夹中还没有谱面。"}</p>}
      {!!query.data?.total && <nav className="collection-pagination" aria-label="谱面分页"><span>{query.data.offset + 1}–{Math.min(query.data.offset + 50, query.data.total)} / {query.data.total}</span><div><button disabled={query.data.offset === 0} onClick={() => update((next) => next.set(pageParam, String(Math.max(0, query.data!.offset / 50 - 1))))}>上一页</button><button disabled={query.data.offset + 50 >= query.data.total} onClick={() => update((next) => next.set(pageParam, String(query.data!.offset / 50 + 1)))}>下一页</button></div></nav>}
    </>}
    {record && <CollectionRecordDrawer key={`${record.folder_id}:${record.entry.id}`} row={record} player={player || null} onClose={() => setRecord(null)} />}
    <Dialog.Root open={!!detail} onOpenChange={(open) => { if (!open) setDetail(null); }}>{detail?.local && <BeatmapDetailDrawer client={detail.local.resource.client} resourceId={detail.local.resource.resource_id} onClose={() => setDetail(null)} />}</Dialog.Root>
    <Dialog.Root open={rename !== null || deleting} onOpenChange={(open) => { if (!open) { setRename(null); setDeleting(false); } }}><Dialog.Portal><Dialog.Overlay className="collection-dialog-overlay" /><Dialog.Content className="collection-modal"><Dialog.Title>{deleting ? "删除收藏夹" : "重命名收藏夹"}</Dialog.Title><Dialog.Description>{deleting ? "同时删除这个收藏夹的标签、笔记和手动成绩。本地谱面文件及游戏成绩不会被删除。" : "修改收藏夹在 OPP 中的名称。"}</Dialog.Description>{rename !== null && <input autoFocus aria-label="收藏夹新名称" value={rename} maxLength={120} onChange={(e) => setRename(e.target.value)} />}<div className="collection-modal-actions"><Dialog.Close disabled={busy}>取消</Dialog.Close><button disabled={busy || (rename !== null && !rename.trim())} onClick={() => void run(async () => { if (!folder) return; if (deleting) { await onChanged(folder.id); setDeleting(false); openFolder(null); } else { await desktopApi.renameCollection(folder.id, rename!.trim()); await onChanged(); setRename(null); } })}>确认{deleting ? "删除" : "修改"}</button></div></Dialog.Content></Dialog.Portal></Dialog.Root>
  </section>;
}
