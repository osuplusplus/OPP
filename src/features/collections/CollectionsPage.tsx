import { PaginatedList } from "../../shared/components/PaginatedList";
import * as Dialog from "@radix-ui/react-dialog";
import { open } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useBeforeUnload, useNavigate } from "react-router-dom";
import {
  FileInput,
  FolderPlus,
  RefreshCw,
  Save,
  X,
} from "lucide-react";
import { Button } from "../../shared/components/ui";
import { NotificationCard } from "../../shared/components/notifications";
import { AppDialog } from "../../shared/components/AppDialog";
import { APP_TIME_ZONE } from "../../shared/lib/format";
import { CollectionExplorer } from "./CollectionExplorer";
import { desktopApi } from "../../shared/lib/tauri";
import type {
  CollectionSharePreview,
  BeatmapDownloadProgress,
  CommandError,
} from "../../shared/types/osu";
import { resolveDefaultDownloadProvider } from "../online-beatmaps/downloadProvider";
import { collectionsQueryKey, collectionSummariesKey, collectionEntriesKey, useCollectionSummaries, useRefreshCollections } from "./api";
import { beginCollectionTask, throwIfCollectionTaskCancelled, updateCollectionTask } from "./taskStatus";

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
                <PaginatedList items={preview.entries} pageSize={50} label="导入预览">{(entries) => <div className="space-y-1.5">
                  {entries.map((entry, index) => (
                    <div className="flex items-center gap-3 rounded-xl border border-white/[0.06] bg-black/10 px-3 py-2.5" key={entry.id}>
                      <span className={`size-2 rounded-full ${entry.resolved ? "bg-emerald-300" : entry.beatmapset_id ? "bg-cyan-300" : "bg-amber-300"}`} />
                      <div className="min-w-0 flex-1">
                        <p className="truncate text-sm text-slate-200">{entry.title || `谱面 #${entry.beatmap_id ?? index + 1}`} <span className="text-slate-500">[{entry.difficulty_name}]</span></p>
                        <p className="truncate text-xs text-slate-600">{entry.beatmapset_id ? `谱面集 #${entry.beatmapset_id} · 难度 #${entry.beatmap_id}` : entry.artist || "本地谱面引用"}</p>
                      </div>
                    </div>
                  ))}
                </div>}</PaginatedList>
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

export function CollectionsPage() {
  const queryClient = useQueryClient();
  const collections = useCollectionSummaries();
  const refreshCollections = useRefreshCollections();
  const [manageOpen, setManageOpen] = useState(false);
  const [name, setName] = useState("");
  const [shareCode, setShareCode] = useState("");
  const [preview, setPreview] = useState<CollectionSharePreview | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const refresh = async (client: "stable" | "lazer") => {
    setBusy(true);
    setNotice(null);
    try {
      await refreshCollections(client);
      setNotice(`已读取 ${client === "lazer" ? "lazer" : "Stable"} 收藏夹。`);
    } catch (caught) {
      setNotice((caught as CommandError).message ?? String(caught));
    } finally {
      setBusy(false);
    }
  };
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
    let active = true;
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
    }).then((unlisten) => { if (active) dispose = unlisten; else unlisten(); });
    return () => { active = false; dispose?.(); };
  }, []);

  const changed = useCallback(async (folderId?: string, entryId?: string) => {
    if (!folderId) {
      void queryClient.invalidateQueries({ queryKey: collectionsQueryKey });
      return;
    }
    try {
      if (entryId) await desktopApi.removeCollectionEntry(folderId, entryId);
      else await desktopApi.deleteCollection(folderId);
    } finally {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: collectionSummariesKey }),
        queryClient.invalidateQueries({ queryKey: collectionEntriesKey(folderId) }),
        queryClient.invalidateQueries({ queryKey: collectionsQueryKey }),
      ]);
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
        setNotice(status.game_changed ? "游戏收藏夹已变更，点击“读取 Stable”将重新读取 Stable 数据。" : "软件收藏夹有待写回的更改。");
      }
      if (status.missing_downloadable_count > 0) {
        setNotice(`收藏夹发现 ${status.missing_downloadable_count} 个缺失谱面集，可点击“补齐并写回 Stable”处理。`);
      }
    }).catch(() => undefined);
    return () => { cancelled = true; };
  }, [collections.data]);
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

  return <>
    <CollectionExplorer folders={collections.data?.folders ?? []} loading={collections.isLoading} failed={collections.isError} onRetry={() => void collections.refetch()} onChanged={changed} onDownload={downloadOneFolder} onNotice={setNotice} toolbar={<>
      <Button onClick={() => setManageOpen(true)} size="sm"><FolderPlus className="size-4" />新建 / 导入</Button>
      <Button disabled={busy} onClick={() => void refresh("stable")} size="sm" variant="secondary"><RefreshCw className="size-3.5" />读取 Stable</Button>
      <Button disabled={busy} onClick={() => void refresh("lazer")} size="sm" variant="secondary"><RefreshCw className="size-3.5" />读取 lazer</Button>
      <Button loading={busy} onClick={() => void writeWithAutoDownload()} size="sm"><Save className="size-3.5" />补齐并写回 Stable</Button>
    </>} />
    {notice ? <NotificationCard className="fixed bottom-5 left-1/2 z-[100] -translate-x-1/2" description={notice} onClose={() => setNotice(null)} title="收藏夹提示" tone="info" /> : null}
    <Dialog.Root open={manageOpen} onOpenChange={setManageOpen}><Dialog.Portal><Dialog.Overlay className="collection-dialog-overlay" /><Dialog.Content className="collection-modal"><Dialog.Title>新建与导入</Dialog.Title><Dialog.Description>创建自己的收藏夹，或导入分享码和谱面压缩包。</Dialog.Description>
      <form onSubmit={(event) => { event.preventDefault(); void create().catch((error) => setNotice(String(error))); }}><label className="text-sm">收藏夹名称<input className="mt-2" value={name} onChange={(e) => setName(e.target.value)} placeholder="我的练习曲包" maxLength={120} /></label><Button className="mt-3" disabled={busy || !name.trim()} type="submit">创建收藏夹</Button></form>
      <div className="mt-6 border-t border-white/10 pt-5"><h3 className="text-sm">导入分享码</h3><textarea className="mt-3 h-24 w-full rounded-lg bg-black/20 p-3 text-xs" value={shareCode} onChange={(e) => { setShareCode(e.target.value); setPreview(null); }} placeholder="粘贴 OPPC2.… 分享码" /><Button disabled={busy || !shareCode.trim()} onClick={() => void importShare()} className="mt-2" variant="secondary">解析分享码</Button></div>
      <div className="mt-6 border-t border-white/10 pt-5"><Button disabled={busy} onClick={() => void importArchive()} variant="secondary"><FileInput className="size-4" />导入压缩包</Button><p className="mt-2 text-xs text-slate-400">支持 .osz 和含 .osu 的 .zip 文件。</p></div>
      <div className="mt-5 text-xs text-slate-400">{collections.data?.sources.map((source) => <p className="mt-2" key={source.client}>{source.client}: {source.message}</p>)}</div>
      <div className="collection-modal-actions"><Dialog.Close>完成</Dialog.Close></div>
    </Dialog.Content></Dialog.Portal></Dialog.Root>
    <ImportPreviewDialog busy={busy} onCancel={() => setPreview(null)} onConfirm={() => void confirmImport()} preview={preview} />
    <AppDialog closeDisabled={busy} description="收藏夹修改已保存在 OPP。离开前是否写回 osu!stable？" footer={<><Button disabled={busy} onClick={stay} variant="ghost">留在此页</Button><Button disabled={busy} onClick={discardAndLeave} variant="secondary">暂不写回</Button><Button loading={busy} onClick={() => void saveAndLeave()}><Save className="size-4" />写回并离开</Button></>} onOpenChange={(open) => { if (!open && !busy) stay(); }} open={leavePrompt} size="sm" title="收藏夹尚未写回游戏"><p className="text-sm leading-6 text-slate-300">选择暂不写回只会离开当前页面，不会丢失 OPP 中的修改。</p></AppDialog>
  </>;
}
