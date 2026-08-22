import * as Dialog from "@radix-ui/react-dialog";
import { open } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useBeforeUnload, useNavigate } from "react-router-dom";
import {
  Download,
  FileInput,
  FileOutput,
  FolderPlus,
  Heart,
  RefreshCw,
  Save,
  Trash2,
  X,
} from "lucide-react";
import { Button, Card, EmptyState } from "../../shared/components/ui";
import { APP_TIME_ZONE } from "../../shared/lib/format";
import { PageHeader } from "../../shared/components/PageHeader";
import { desktopApi } from "../../shared/lib/tauri";
import type {
  CollectionFolder,
  CollectionSnapshot,
  CollectionSharePreview,
  BeatmapDownloadProgress,
  CommandError,
} from "../../shared/types/osu";
import { resolveDefaultDownloadProvider } from "../online-beatmaps/downloadProvider";
import { collectionsQueryKey, removeFromCollectionsSnapshot, useCollections, useRefreshCollections } from "./api";
import { MapCard } from "./MapCard";
import { beginCollectionTask, throwIfCollectionTaskCancelled, updateCollectionTask } from "./taskStatus";

async function copy(value: string) {
  if (navigator.clipboard?.writeText) await navigator.clipboard.writeText(value);
}

function ImportPreviewDialog({
  preview,
  busy,
  onCancel,
  onConfirm,
}: {
  preview: CollectionSharePreview | null;
  busy: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  return (
    <Dialog.Root onOpenChange={(open) => !open && onCancel()} open={Boolean(preview)}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-[260] bg-black/70 backdrop-blur-sm" />
        <Dialog.Content className="fixed left-1/2 top-1/2 z-[270] flex max-h-[min(760px,calc(100vh-32px))] w-[min(720px,calc(100vw-32px))] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-2xl border border-cyan-300/20 bg-[#101724] shadow-2xl outline-none">
          {preview ? (
            <>
              <div className="flex items-start justify-between gap-4 border-b border-white/[0.08] p-6">
                <div>
                  <Dialog.Title className="text-lg font-semibold text-white">导入预览：{preview.name}</Dialog.Title>
                  <Dialog.Description className="mt-1 text-sm text-slate-400">
                    创建者 {preview.creator || "未署名"} · {new Intl.DateTimeFormat("zh-CN", { dateStyle: "medium", timeStyle: "short", timeZone: APP_TIME_ZONE }).format(new Date(preview.created_at))}
                  </Dialog.Description>
                </div>
                <Dialog.Close className="text-slate-500 hover:text-white"><X className="size-5" /></Dialog.Close>
              </div>
              <div className="grid grid-cols-3 gap-3 border-b border-white/[0.08] p-5 text-center text-sm">
                <span className="rounded-lg bg-black/15 p-3 text-slate-200">{preview.entries.length}<small className="mt-1 block text-slate-500">难度</small></span>
                <span className="rounded-lg bg-black/15 p-3 text-emerald-200">{preview.downloadable_count}<small className="mt-1 block text-slate-500">可下载</small></span>
                <span className="rounded-lg bg-black/15 p-3 text-amber-200">{preview.unresolved_count}<small className="mt-1 block text-slate-500">无法下载</small></span>
              </div>
              <div className="min-h-0 flex-1 overflow-y-auto p-4">
                <p className="mb-3 text-xs text-slate-500">将导入以下难度（在线分享码为保持短小，只保存精确谱面 ID）：</p>
                <div className="space-y-1.5">
                  {preview.entries.map((entry, index) => (
                    <div className="flex items-center gap-3 rounded-xl border border-white/[0.06] bg-black/10 px-3 py-2.5" key={entry.id}>
                      <span className={`size-2 rounded-full ${entry.resolved ? "bg-emerald-300" : entry.beatmapset_id ? "bg-cyan-300" : "bg-amber-300"}`} />
                      <div className="min-w-0 flex-1">
                        <p className="truncate text-sm text-slate-200">{entry.title || `谱面 #${entry.beatmap_id ?? index + 1}`} <span className="text-slate-500">[{entry.difficulty_name}]</span></p>
                        <p className="truncate text-xs text-slate-600">{entry.beatmapset_id ? `谱面集 #${entry.beatmapset_id} · 难度 #${entry.beatmap_id}` : entry.artist || "本地谱面引用"}</p>
                      </div>
                    </div>
                  ))}
                </div>
              </div>
              <div className="flex justify-end gap-2 border-t border-white/[0.08] p-5">
                <Button disabled={busy} onClick={onCancel} variant="ghost">取消</Button>
                <Button loading={busy} onClick={onConfirm}>{preview.downloadable_count ? "导入并自动补齐" : "确认导入为新收藏夹"}</Button>
              </div>
            </>
          ) : null}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

function FolderCard({ folder, onChanged, onDownload }: { folder: CollectionFolder; onChanged: (folderId: string, entryId?: string) => Promise<void>; onDownload: (folderId: string) => Promise<void> }) {
  const [exported, setExported] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const remove = async (entryId?: string) => {
    setBusy(true);
    try {
      await onChanged(folder.id, entryId);
    } catch (caught) { setError((caught as CommandError).message ?? String(caught)); }
    finally { setBusy(false); }
  };
  const exportShare = async () => {
    setBusy(true);
    try {
      const code = await desktopApi.exportCollectionShare(folder.id, folder.creator);
      setExported(code);
      await copy(code);
    } catch (caught) { setError((caught as CommandError).message ?? String(caught)); }
    finally { setBusy(false); }
  };
  const download = async () => {
    setBusy(true);
    try {
      await onDownload(folder.id);
    } catch (caught) { setError((caught as CommandError).message ?? String(caught)); }
    finally { setBusy(false); }
  };
  const missingSets = new Set(folder.entries.filter((entry) => !entry.resolved && (entry.beatmapset_id || entry.checksum)).map((entry) => entry.beatmapset_id ?? entry.checksum)).size;

  return (
    <Card className="collection-folder overflow-hidden">
      <div className="flex items-start justify-between gap-4 border-b border-white/[0.07] p-5">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <h2 className="truncate text-base font-semibold text-white">{folder.name}</h2>
            {folder.read_only ? <span className="rounded bg-amber-300/10 px-2 py-0.5 text-xs text-amber-200">只读</span> : null}
            {folder.pending_write ? <span className="rounded bg-cyan-300/10 px-2 py-0.5 text-xs text-cyan-100">待写回</span> : null}
          </div>
          <p className="mt-1 text-xs text-slate-500">{folder.creator || "未署名"} · {folder.entries.length} 个难度{missingSets ? ` · 缺失 ${missingSets} 项` : ""}</p>
        </div>
        <div className="flex gap-1">
          <Button disabled={busy} onClick={() => void exportShare()} size="icon" title="导出分享码" variant="ghost"><FileOutput className="size-4" /></Button>
          <Button disabled={busy || folder.read_only} onClick={() => void download()} size="icon" title={missingSets ? `解析并补齐 ${missingSets} 个缺失项` : "检查是否有缺失谱面"} variant="ghost"><Download className="size-4" /></Button>
          <Button disabled={busy || folder.read_only} onClick={() => void remove()} size="icon" title="删除收藏夹" variant="ghost"><Trash2 className="size-4" /></Button>
        </div>
      </div>
      <div className="max-h-[34rem] overflow-y-auto p-3.5">
        {folder.entries.length ? (
          <div className="grid grid-cols-[repeat(auto-fit,minmax(min(100%,18rem),1fr))] gap-2.5">
            {/* 网格只控制可读宽度与换列时机，卡片高度始终由宽高比例决定。 */}
            {folder.entries.map((entry) => (
              <MapCard
                busy={busy}
                entry={entry}
                key={entry.id}
                onRemove={() => void remove(entry.id)}
                readOnly={folder.read_only}
              />
            ))}
          </div>
        ) : <p className="px-5 py-8 text-center text-sm text-slate-600">还没有谱面</p>}
      </div>
      {exported ? <div className="border-t border-white/[0.07] p-4"><p className="mb-2 text-xs text-emerald-200">分享码已复制。OPPC2 会紧凑保存在线谱面 ID。</p><textarea className="h-20 w-full rounded-lg border border-white/10 bg-black/20 p-2 font-mono text-[10px] text-slate-400" readOnly value={exported} /></div> : null}
      {error ? <p className="p-4 text-sm text-rose-200">{error}</p> : null}
    </Card>
  );
}

export function CollectionsPage() {
  const queryClient = useQueryClient();
  const collections = useCollections();
  const refresh = useRefreshCollections();
  const [name, setName] = useState("");
  const [shareCode, setShareCode] = useState("");
  const [preview, setPreview] = useState<CollectionSharePreview | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const collectionDownloadActive = useRef(false);
  const [leavePrompt, setLeavePrompt] = useState(false);
  const [pendingNavigation, setPendingNavigation] = useState<string | null>(null);
  const navigate = useNavigate();
  const hasUnsavedChanges = collections.data?.folders.some((folder) => folder.pending_write) ?? false;

  useEffect(() => {
    const interceptNavigation = (event: MouseEvent) => {
      if (!hasUnsavedChanges || event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey) return;
      const anchor = (event.target as HTMLElement | null)?.closest<HTMLAnchorElement>("a[href]");
      const target = anchor?.getAttribute("href");
      if (!anchor || anchor.target || anchor.download || !target?.startsWith("/") || target === `${window.location.pathname}${window.location.search}${window.location.hash}`) return;
      event.preventDefault();
      event.stopPropagation();
      setPendingNavigation(target);
      setLeavePrompt(true);
    };
    window.addEventListener("click", interceptNavigation, true);
    return () => window.removeEventListener("click", interceptNavigation, true);
  }, [hasUnsavedChanges]);
  useBeforeUnload((event) => {
    if (!hasUnsavedChanges) return;
    event.preventDefault();
    event.returnValue = "";
  });

  useEffect(() => {
    let dispose: (() => void) | undefined;
    void desktopApi.onBeatmapDownloadProgress((progress: BeatmapDownloadProgress) => {
      if (!collectionDownloadActive.current) return;
      if (progress.phase === "finished") {
        setNotice(`曲包下载完成（成功 ${progress.completed}、跳过 ${progress.skipped}、失败 ${progress.failed}），正在准备安装…`);
        return;
      }
      if (progress.phase === "cancelled") {
        setNotice("缺失曲包下载已取消。");
        return;
      }
      const current = Math.min(progress.total, progress.processed + (progress.phase === "downloading" ? 1 : 0));
      setNotice(`正在下载缺失曲包 ${current}/${progress.total}${progress.current_title ? `：${progress.current_title}` : ""}`);
    }).then((unlisten) => { dispose = unlisten; });
    return () => dispose?.();
  }, []);

  const changed = useCallback(async (folderId?: string, entryId?: string) => {
    if (!folderId) {
      void queryClient.invalidateQueries({ queryKey: collectionsQueryKey });
      return;
    }
    await queryClient.cancelQueries({ queryKey: collectionsQueryKey });
    const previous = queryClient.getQueryData<CollectionSnapshot>(collectionsQueryKey);
    queryClient.setQueryData<CollectionSnapshot>(
      collectionsQueryKey,
      (snapshot) => removeFromCollectionsSnapshot(snapshot, folderId, entryId),
    );
    try {
      if (entryId) await desktopApi.removeCollectionEntry(folderId, entryId);
      else await desktopApi.deleteCollection(folderId);
    } catch (caught) {
      queryClient.setQueryData(collectionsQueryKey, previous);
      throw caught;
    } finally {
      void queryClient.invalidateQueries({ queryKey: collectionsQueryKey });
    }
  }, [queryClient]);
  const downloadFoldersToGame = useCallback(async (folderIds: string[]) => {
    await desktopApi.beginCollectionTask();
    setNotice("正在检查收藏夹中的缺失谱面…");
    beginCollectionTask({ phase: "checking", message: "正在检查收藏夹中的缺失谱面和旧 MD5…", processed: 0, total: 0, errors: [] });
    const detectedItems = await desktopApi.getCollectionDownloadItems(folderIds);
    throwIfCollectionTaskCancelled();
    const items = [...new Map(detectedItems.map((item) => [item.beatmapset_id, item])).values()];
    if (!items.length) {
      updateCollectionTask({ phase: "completed", message: "没有需要下载的缺失曲包" });
      return null;
    }

    const stable = (await desktopApi.getLocalSources()).find((source) => source.client === "stable");
    if (!stable?.valid || !stable.install_root) {
      throw new Error("请先在设置中配置有效的 osu!stable 安装目录，才能将线上谱面自动下载到游戏。");
    }

    const settings = await desktopApi.getSettings();
    const root = stable.install_root.replace(/[\\/]+$/, "");
    const destination = settings.beatmap_download_directory || `${root}\\OPP Downloads`;
    collectionDownloadActive.current = true;
    setNotice(`准备下载 ${items.length} 个缺失曲包…`);
    updateCollectionTask({ phase: "downloading", message: `准备下载 ${items.length} 个缺失曲包…`, processed: 0, total: items.length });
    try {
      const download = await desktopApi.downloadOnlineBeatmapsets({
        destination,
        provider: resolveDefaultDownloadProvider(settings),
        overwrite: false,
        include_video: settings.include_video_in_beatmap_downloads,
        open_after_download: false,
        items,
      });
      throwIfCollectionTaskCancelled();
      const archivePaths = download.completed_paths ?? [];
      if (!archivePaths.length) {
        throw new Error(download.failed ? "缺失曲包下载失败，请检查下载源后重试。" : "没有找到可用于补齐收藏夹的曲包文件。");
      }
      setNotice(`正在统一读取 ${archivePaths.length} 个曲包，并计算收藏难度 MD5…`);
      updateCollectionTask({
        phase: "installing",
        message: `正在读取 ${archivePaths.length} 个曲包并计算收藏难度 MD5…`,
        processed: 0,
        total: archivePaths.length,
        errors: download.failures.map((failure) => `#${failure.beatmapset_id} ${failure.title}：${failure.message}`),
      });
      const install = await desktopApi.installCollectionDownloads(folderIds, archivePaths);
      throwIfCollectionTaskCancelled();
      changed();
      return { download, install };
    } finally {
      collectionDownloadActive.current = false;
    }
  }, [changed]);
  const downloadMissingBeatmapsToGame = useCallback(() => downloadFoldersToGame((collections.data?.folders ?? []).filter((folder) => folder.source !== "lazer").map((folder) => folder.id)), [collections.data?.folders, downloadFoldersToGame]);
  const finalizeCollections = useCallback(async (completed: Awaited<ReturnType<typeof downloadFoldersToGame>>) => {
    setBusy(true);
    try {
      throwIfCollectionTaskCancelled();
      setNotice("正在安全写回 osu!stable/collection.db…");
      updateCollectionTask({ phase: "writing", message: "曲包 MD5 已准备完成，正在统一写回 collection.db…", processed: 0, total: 1 });
      const written = await desktopApi.writeStableCollections();
      throwIfCollectionTaskCancelled();
      if (!completed) {
        const message = `已写回 ${written.written_folders} 个收藏夹，没有需要导入的缺失曲包。`;
        setNotice(message);
        updateCollectionTask({ phase: "completed", message, processed: 1, total: 1 });
        changed();
        return true;
      }
      const archivePaths = completed.download.completed_paths ?? [];
      updateCollectionTask({ phase: "opening", message: `collection.db 已写回，正在交给 osu! 导入 ${archivePaths.length} 个曲包…`, processed: 0, total: archivePaths.length });
      const opened = await desktopApi.openCollectionDownloads(archivePaths);
      throwIfCollectionTaskCancelled();
      const errors = [
        ...completed.download.failures.map((failure) => `#${failure.beatmapset_id} ${failure.title}：${failure.message}`),
        ...opened.failures,
      ];
      const message = `已写回 ${written.written_folders} 个收藏夹，并交给 osu! 导入 ${opened.opened} 个曲包${errors.length ? `；${errors.length} 项失败` : ""}。`;
      setNotice(message);
      updateCollectionTask({ phase: errors.length ? "failed" : "completed", message, processed: opened.opened + opened.failed, total: archivePaths.length, errors });
      changed();
      return errors.length === 0;
    } catch (caught) {
      const message = (caught as CommandError).message ?? String(caught);
      setNotice(message);
      updateCollectionTask({ phase: "failed", message: "写回或调用游戏导入失败", errors: [message] });
      return false;
    } finally {
      setBusy(false);
    }
  }, [changed]);
  const downloadOneFolder = async (folderId: string) => {
    try {
      const result = await downloadFoldersToGame([folderId]);
      await finalizeCollections(result);
    } catch (caught) {
      const message = (caught as CommandError).message ?? String(caught);
      updateCollectionTask({ phase: "failed", message: "收藏夹补齐失败", errors: [message] });
      throw caught;
    }
  };

  useEffect(() => {
    if (!collections.data) return;
    let cancelled = false;
    void desktopApi.getCollectionSyncStatus().then((status) => {
      if (cancelled) return;
      if (!status.in_sync) {
        setNotice(status.game_changed ? "游戏收藏夹已变更，点击“读取本地”将重新读取 Stable 数据。" : "软件收藏夹有待写回的更改。");
      }
      if (status.missing_downloadable_count > 0 && window.confirm(`收藏夹发现 ${status.missing_downloadable_count} 个缺失谱面集，是否由 OPP 批量下载到 osu!stable？`)) {
        void downloadMissingBeatmapsToGame().then(async (result) => {
          if (!cancelled) await finalizeCollections(result);
        }).catch((caught: unknown) => {
          if (!cancelled) {
            const message = (caught as CommandError).message ?? String(caught);
            setNotice(message);
            updateCollectionTask({ phase: "failed", message: "收藏夹自动补齐失败", errors: [message] });
          }
        });
      }
    }).catch(() => undefined);
    return () => { cancelled = true; };
  }, [collections.data, downloadMissingBeatmapsToGame, finalizeCollections]);
  const create = async () => { if (!name.trim()) return; setBusy(true); try { await desktopApi.createCollection(name.trim(), ""); setName(""); changed(); } finally { setBusy(false); } };
  const importShare = async () => { setBusy(true); try { setPreview(await desktopApi.previewCollectionShare(shareCode)); } catch (caught) { setNotice((caught as CommandError).message ?? String(caught)); } finally { setBusy(false); } };
  const importArchive = async () => {
    const path = await open({ multiple: false, filters: [{ name: "谱面压缩包", extensions: ["osz", "zip"] }] });
    if (typeof path !== "string") return;
    setBusy(true);
    try {
      const folder = await desktopApi.importCollectionArchive(path);
      await queryClient.invalidateQueries({ queryKey: collectionsQueryKey });
      setNotice(`已从压缩包创建“${folder.name}”，导入 ${folder.entries.length} 个难度。`);
    } catch (caught) { setNotice((caught as CommandError).message ?? String(caught)); }
    finally { setBusy(false); }
  };
  const confirmImport = async () => { setBusy(true); try { const shouldDownload = Boolean(preview?.downloadable_count); const imported = await desktopApi.importCollectionShare(shareCode); setShareCode(""); setPreview(null); changed(); if (shouldDownload) { const result = await downloadFoldersToGame([imported.id]); await finalizeCollections(result); } else { const message = `已导入“${imported.name}”，包含 ${imported.entries.length} 个难度。`; setNotice(message); updateCollectionTask({ phase: "completed", message }); } } catch (caught) { const message = (caught as CommandError).message ?? String(caught); setNotice(message); updateCollectionTask({ phase: "failed", message: "分享码导入补齐失败", errors: [message] }); } finally { setBusy(false); } };
  const writeWithAutoDownload = async () => {
    setBusy(true);
    try {
      const completed = await downloadMissingBeatmapsToGame();
      return await finalizeCollections(completed);
    } catch (caught) {
      const message = (caught as CommandError).message ?? String(caught);
      setNotice(message);
      updateCollectionTask({ phase: "failed", message: "收藏夹同步失败", errors: [message] });
      return false;
    } finally {
      setBusy(false);
    }
  };
  const completeLeave = () => { const target = pendingNavigation; setLeavePrompt(false); setPendingNavigation(null); if (target) navigate(target); };
  const saveAndLeave = async () => { if (await writeWithAutoDownload()) completeLeave(); };
  const discardAndLeave = completeLeave;
  const stay = () => { setLeavePrompt(false); setPendingNavigation(null); };

  return <><PageHeader title="谱面收藏夹" description="统一管理游戏收藏夹与 OPP 分享图包；Stable 支持自动补齐并安全写回，lazer 当前只读。" actions={<div className="flex gap-2"><Button disabled={busy} onClick={() => void importArchive()} size="sm" variant="secondary"><FileInput className="size-3.5" />导入压缩包</Button><Button disabled={busy} onClick={() => void refresh("stable")} size="sm" variant="secondary"><RefreshCw className="size-3.5" />读取本地</Button><Button loading={busy} onClick={() => void writeWithAutoDownload()} size="sm"><Save className="size-3.5" />补齐并写回游戏</Button></div>} /><div className="grid gap-5 2xl:grid-cols-[minmax(0,1fr)_360px]"><section className="space-y-4">{collections.isLoading ? <p className="text-sm text-slate-500">正在读取收藏夹…</p> : collections.data?.folders.length ? collections.data.folders.map((folder) => <FolderCard folder={folder} key={folder.id} onChanged={changed} onDownload={downloadOneFolder} />) : <EmptyState icon={<Heart className="size-6" />} title="还没有收藏夹" description="从在线、本地或相似谱面页将难度加入收藏夹，或导入 .osz/.zip。" />}</section><aside className="space-y-4"><Card className="p-5"><h2 className="text-sm font-semibold text-white">新建收藏夹</h2><input className="opp-input mt-3" onChange={(event) => setName(event.target.value)} placeholder="收藏夹名称" value={name} /><Button className="mt-3 w-full" disabled={busy || !name.trim()} onClick={() => void create()}><FolderPlus className="size-4" />创建</Button></Card><Card className="p-5"><h2 className="flex items-center gap-2 text-sm font-semibold text-white"><FileInput className="size-4 text-cyan-200" />导入分享码</h2><textarea className="mt-3 h-28 w-full rounded-xl border border-white/10 bg-black/20 p-3 font-mono text-xs text-slate-300" onChange={(event) => { setShareCode(event.target.value); setPreview(null); }} placeholder="粘贴 OPPC2.… 分享码" value={shareCode} /><Button className="mt-3 w-full" disabled={busy || !shareCode.trim()} onClick={() => void importShare()} variant="secondary">解析分享码</Button></Card><Card className="p-5"><h2 className="text-sm font-semibold text-white">从谱面包导入</h2><p className="mt-2 text-xs leading-5 text-slate-500">选择 `.osz` 或包含 `.osu` 文件的 `.zip`，自动创建同名收藏夹。</p><Button className="mt-3 w-full" disabled={busy} onClick={() => void importArchive()} variant="secondary"><FileInput className="size-4" />选择压缩包</Button></Card><Card className="p-5"><h2 className="text-sm font-semibold text-white">游戏来源</h2>{collections.data?.sources.map((source) => <div className="mt-3 border-t border-white/[0.06] pt-3" key={source.client}><p className="text-sm text-slate-200">osu! {source.client}{source.read_only ? " · 只读" : ""}</p><p className="mt-1 text-xs leading-5 text-slate-500">{source.message}</p></div>)}</Card>{notice ? <p aria-live="polite" className="rounded-xl border border-cyan-300/15 bg-cyan-300/[0.06] p-4 text-sm text-cyan-100">{notice}</p> : null}</aside></div><ImportPreviewDialog busy={busy} onCancel={() => setPreview(null)} onConfirm={() => void confirmImport()} preview={preview} />{leavePrompt ? <div className="fixed inset-0 z-[280] grid place-items-center bg-black/70 p-5 backdrop-blur-sm"><Card className="w-full max-w-md p-6 shadow-2xl"><h2 className="text-lg font-semibold text-white">收藏夹尚未写回游戏</h2><p className="mt-2 text-sm leading-6 text-slate-400">你对收藏夹做了修改。离开前是否保存到 osu!stable？</p><div className="mt-6 flex flex-wrap justify-end gap-2"><Button disabled={busy} onClick={stay} variant="ghost">留在此页</Button><Button disabled={busy} onClick={discardAndLeave} variant="secondary">不保存并离开</Button><Button loading={busy} onClick={() => void saveAndLeave()}><Save className="size-4" />保存并离开</Button></div></Card></div> : null}</>;
}
