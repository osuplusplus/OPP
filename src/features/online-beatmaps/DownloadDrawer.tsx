import * as Dialog from "@radix-ui/react-dialog";
import { useState } from "react";
import { Download, FolderOpen, X } from "lucide-react";
import { desktopApi } from "../../shared/lib/tauri";
import { byteSize, errorMessage } from "../../shared/lib/format";
import type { BeatmapDownloadProgress, BeatmapDownloadProvider } from "../../shared/types/osu";
import { downloadSession } from "./downloadSession";
import { useOnlineDownload } from "./useOnlineDownload";
import { useSettings } from "../settings/api";
import { DownloadResultActions } from "./DownloadResultActions";

function itemStatus(progress: BeatmapDownloadProgress | undefined, active: boolean, cancelled: boolean) {
  if (progress?.phase === "completed") return "已完成";
  if (progress?.phase === "skipped") return "已跳过";
  if (progress?.phase === "failed") return "下载失败";
  if (progress?.phase === "downloading") return "下载中";
  if (active) return "等待下载";
  if (cancelled) return "已取消";
  return "待下载";
}

function itemProgressText(progress: BeatmapDownloadProgress) {
  if (progress.phase === "failed") return progress.message || "下载失败";
  if (progress.phase === "completed") return "已保存到下载目录";
  if (progress.phase === "skipped") return "文件已存在，已跳过";
  return `${byteSize(progress.downloaded_bytes)}${progress.total_bytes ? ` / ${byteSize(progress.total_bytes)}` : ""}`;
}

function DownloadItemProgress({ progress, title }: { progress: BeatmapDownloadProgress; title: string }) {
  const hasTotal = typeof progress.total_bytes === "number" && progress.total_bytes > 0;
  const downloaded = progress.downloaded_bytes ?? 0;
  const speed = `${byteSize(progress.bytes_per_second ?? 0)}/s`;
  return <div className="online-download-item-progress">
    <div className="online-download-item-progress-meta"><span>{itemProgressText(progress)}</span><strong>{progress.phase === "downloading" ? speed : itemStatus(progress, false, false)}</strong></div>
    {hasTotal ? <progress aria-label={`${title}下载进度`} max={progress.total_bytes!} value={Math.min(downloaded, progress.total_bytes!)} /> : null}
  </div>;
}

function DownloadContents() {
  const { state, start, destination, defaultProvider, saveDestination } = useOnlineDownload();
  const [path, setPath] = useState(destination);
  const [provider, setProvider] = useState<BeatmapDownloadProvider | "none">(defaultProvider);
  const [overwrite, setOverwrite] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [choosing, setChoosing] = useState(false);
  const progress = state.displayProgress ?? state.progress;
  const choose = async () => {
    setChoosing(true); setError(null);
    try { const selected = await desktopApi.chooseBeatmapDownloadDirectory(path || null); if (selected) { await saveDestination(selected); setPath(selected); } }
    catch (caught) { setError(errorMessage(caught)); }
    finally { setChoosing(false); }
  };
  const pending = state.queue.filter((item) => !state.activeIds.includes(item.id));
  return <>
    <div className="online-download-body">
    <div className="online-download-summary"><span>{state.queue.length} 个谱面集</span><button disabled={!pending.length} onClick={() => downloadSession.clear()}>清空待下载</button></div>
    <div className="online-download-items">
      {!state.queue.length ? <p className="online-empty">清单为空。可从搜索结果多选添加，也可以按筛选条件批量加入。</p> : state.queue.map((item) => {
        const progress = state.itemProgress[item.id];
        const active = state.activeIds.includes(item.id);
        return <div key={item.id}>
          <div className="online-download-item-heading">
            <span><strong>{item.title_unicode || item.title}</strong><small>{item.artist_unicode || item.artist} · {item.creator}</small></span>
            <span className={`online-download-item-status is-${progress?.phase ?? (active ? "queued" : "pending")}`}>{itemStatus(progress, active, Boolean(state.result?.cancelled))}</span>
            <button aria-label={`移除 ${item.title}`} disabled={active} onClick={() => downloadSession.remove(item.id)}><X /></button>
          </div>
          {progress ? <DownloadItemProgress progress={progress} title={item.title_unicode || item.title} /> : active ? <div className="online-download-item-progress online-download-item-preparing"><span>正在准备下载…</span></div> : null}
        </div>;
      })}
    </div>
    <div className="online-download-options"><span>保存到</span><p title={path}>{path || "开始下载时选择目录"}</p>
      <details><summary>更多选项</summary>
        <button disabled={state.busy || choosing} onClick={() => void choose()}><FolderOpen />更改保存目录</button>
        <label>下载源<select disabled={state.busy} value={provider} onChange={(event) => setProvider(event.target.value as BeatmapDownloadProvider | "none")}><option value="sayobot">小夜 Sayobot</option><option value="hinai">Hinai（仅手动）</option><option value="catboy">Catboy</option><option value="nerinyan">Nerinyan</option><option value="none">不使用镜像</option></select></label>
        <label><input type="checkbox" disabled={state.busy} checked={overwrite} onChange={(event) => setOverwrite(event.target.checked)} />覆盖同名 .osz 文件</label>
      </details>
    </div>
    {progress ? <div className="online-download-progress" role="status"><span>{progress.current_title || progress.message || "准备下载"}</span><strong>{progress.processed}/{progress.total}</strong><progress max={progress.total || 1} value={progress.processed} /><small>{state.busy ? "下载中，可关闭清单继续浏览" : state.result?.cancelled ? "已取消，未完成项已保留" : "本批次已结束"}</small></div> : null}
    {state.result ? <p role="status">完成 {state.result.completed} · 跳过 {state.result.skipped} · 失败 {state.result.failed}{state.result.cancelled ? " · 已取消" : ""}</p> : null}
    {state.result?.failures.map((failure) => <p className="online-notice" key={failure.beatmapset_id}>{failure.title}：{failure.message}</p>)}
    {state.result && <DownloadResultActions result={state.result} />}
    {error || state.error ? <p className="online-notice" role="alert">{error || state.error}</p> : null}
    </div>
    <footer>{state.busy ? <button onClick={() => void downloadSession.cancel()}>取消本批下载</button> : <button className="is-primary" disabled={!state.queue.length || choosing || provider === "none"} onClick={() => void start(state.queue, { destination: path, provider, overwrite })}><Download />开始下载 · {state.queue.length}</button>}</footer>
  </>;
}

export function DownloadDrawer() {
  const { state } = useOnlineDownload();
  const settings = useSettings();
  const progress = state.displayProgress ?? state.progress;
  return <Dialog.Root><Dialog.Trigger asChild><button className="online-download-trigger" data-page-guide-online-download="true"><Download />{state.busy ? `↓ ${progress?.processed ?? 0}/${progress?.total ?? state.activeIds.length}` : `下载清单 · ${state.queue.length}`}{state.error ? " !" : ""}</button></Dialog.Trigger>
    <Dialog.Portal><Dialog.Overlay className="online-dialog-overlay" /><Dialog.Content data-dialog-layout="drawer" data-reduce-motion={settings.data?.reduce_motion || undefined} className="online-dialog online-download-drawer">
      <Dialog.Title>下载清单</Dialog.Title><Dialog.Description>跨搜索保留所选谱面，关闭清单后下载继续。</Dialog.Description><Dialog.Close className="online-dialog-close" aria-label="关闭下载清单"><X /></Dialog.Close>
      <DownloadContents />
    </Dialog.Content></Dialog.Portal>
  </Dialog.Root>;
}
