import * as Dialog from "@radix-ui/react-dialog";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Download, RefreshCw, X } from "lucide-react";
import { Button, Select } from "../../shared/components/ui";
import { desktopApi } from "../../shared/lib/tauri";
import { errorMessage } from "../../shared/lib/format";
import type { BeatmapDownloadProvider, TournamentPoolRef } from "../../shared/types/osu";
import { useBeatmapDownloads } from "../online-beatmaps/api";
import { useSyncTournamentPool, useTournamentPool } from "./api";
import { poolDownloads, poolTitle } from "./model";

export default function TournamentPoolDialog({ reference, onClose }: { reference: TournamentPoolRef; onClose: () => void }) {
  const query = useTournamentPool(reference);
  const sync = useSyncTournamentPool(reference);
  const downloads = useBeatmapDownloads();
  const navigate = useNavigate();
  const [providerOverride, setProviderOverride] = useState<BeatmapDownloadProvider | "none" | null>(null);
  const [choosing, setChoosing] = useState(false);
  const [directoryError, setDirectoryError] = useState<string | null>(null);
  const provider = providerOverride ?? downloads.defaultProvider;
  const { items, unavailable } = poolDownloads(query.data);
  const busy = sync.isPending || query.isFetching;
  const chooseDirectory = async () => {
    setChoosing(true); setDirectoryError(null);
    try {
      const path = await desktopApi.chooseBeatmapDownloadDirectory(downloads.destination || null);
      if (path) await downloads.saveDestination(path);
    } catch (error) { setDirectoryError(errorMessage(error)); }
    finally { setChoosing(false); }
  };
  return <Dialog.Root open onOpenChange={(open) => { if (!open && !sync.isPending) onClose(); }}>
    <Dialog.Portal>
      <Dialog.Overlay className="fixed inset-0 z-[240] bg-black/65" />
      <Dialog.Content className="fixed left-1/2 top-1/2 z-[241] flex max-h-[85vh] w-[min(56rem,94vw)] -translate-x-1/2 -translate-y-1/2 flex-col rounded-2xl border border-white/10 bg-[var(--surface)] text-slate-100 shadow-2xl">
        <div className="flex items-start justify-between gap-4 border-b border-white/10 p-5">
          <div><Dialog.Title className="text-lg font-semibold">{poolTitle(reference)}</Dialog.Title>
            <Dialog.Description className="mt-1 text-sm text-slate-400">osu!standard 比赛图池 · 选图资料来自 Rino</Dialog.Description></div>
          <Dialog.Close asChild><Button size="icon" variant="ghost" disabled={sync.isPending} aria-label="关闭比赛图池"><X className="size-4" /></Button></Dialog.Close>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto p-5">
          {query.isFetching && <p role="status" className="mb-3 text-sm text-slate-400">正在读取图池和谱面资料…</p>}
          {query.error && <p role="alert" className="mb-3 text-sm text-rose-200">{errorMessage(query.error)}</p>}
          {query.data?.entries.length === 0 && <p className="py-8 text-center text-slate-400">本阶段暂未发布图池</p>}
          <ul className="space-y-3">{query.data?.entries.map((entry) => <li key={`${entry.selection_type}:${entry.position}`} className="rounded-xl border border-white/10 p-3">
            <div className="flex items-start gap-3">
              <span className="min-w-12 rounded bg-cyan-300/10 p-2 text-center text-sm font-semibold text-cyan-200">{entry.selection_type}{entry.position}</span>
              <div className="min-w-0 flex-1">
                <button className="text-left text-sm font-semibold hover:underline" disabled={sync.isPending} onClick={() => { navigate(`/online/beatmaps?${entry.beatmap ? `beatmapset=${entry.beatmap.beatmapset_id}&` : ""}beatmap=${entry.beatmap_id}`); onClose(); }}>
                  {entry.beatmap ? `${entry.beatmap.artist} - ${entry.beatmap.title} [${entry.beatmap.difficulty_name}]` : `Beatmap #${entry.beatmap_id}`}
                </button>
                <p className="mt-1 text-xs text-slate-400">BID {entry.beatmap_id}{entry.beatmap ? ` · 谱师 ${entry.beatmap.creator}` : ""} · 选图 {entry.selected_by_name || entry.selected_by || "未署名"}</p>
                {(entry.is_custom || entry.is_original) && <p className="mt-1 text-xs text-amber-200">{[entry.is_custom && "比赛定制", entry.is_original && "原创"].filter(Boolean).join(" · ")}</p>}
                {entry.comment && <p className="mt-2 whitespace-pre-wrap break-words text-sm text-slate-300">{entry.comment}</p>}
                {entry.resolution_error && <p className="mt-1 text-xs text-amber-200">{entry.resolution_error}；可刷新重试，仍可按 BID 收藏。</p>}
              </div>
            </div>
          </li>)}</ul>
          {unavailable.length > 0 && <p className="mt-3 text-sm text-amber-200">以下谱面资料缺失或不允许下载，将跳过：{unavailable.map((id) => `BID ${id}`).join("、")}</p>}
        </div>
        <footer className="space-y-3 border-t border-white/10 p-5 text-sm">
          <p className="text-xs text-amber-200">同步将替换本阶段收藏夹，移除旧图及手动添加的图。不会自动写回 Stable／lazer。</p>
          {sync.error && <p role="alert" className="text-rose-200">{errorMessage(sync.error)}</p>}
          {sync.data && <p role="status" className="text-emerald-200">已同步 {sync.data.entry_count} 个难度到收藏夹。</p>}
          <details><summary className="cursor-pointer text-slate-400">下载设置 · {downloads.destination || "开始下载时选择目录"}</summary>
            <div className="mt-2 flex items-center gap-3">
              <label className="flex items-center gap-2">下载源<Select aria-label="比赛图池下载源" disabled={downloads.state.busy} value={provider} onChange={(event) => setProviderOverride(event.target.value as BeatmapDownloadProvider | "none")}>
                <option value="sayobot">小夜 Sayobot</option><option value="hinai">Hinai Mirror</option><option value="catboy">Catboy</option><option value="nerinyan">Nerinyan</option><option value="none">不使用镜像</option>
              </Select></label>
              <Button size="sm" disabled={downloads.state.busy || choosing} onClick={() => void chooseDirectory()}>更改保存目录</Button>
            </div>
          </details>
          {provider === "none" && <p className="text-amber-200">请选择下载源后再下载。</p>}
          {(downloads.state.error || directoryError) && <p role="alert" className="text-rose-200">{downloads.state.error || directoryError}</p>}
          {downloads.state.progress && <p role="status">当前下载任务：{downloads.state.progress.processed}/{downloads.state.progress.total} · {downloads.state.progress.current_title || downloads.state.progress.message}</p>}
          {downloads.state.result && <p role="status">{downloads.state.result.cancelled ? "下载已取消" : "下载结束"} · 完成 {downloads.state.result.completed} · 跳过 {downloads.state.result.skipped} · 失败 {downloads.state.result.failed}</p>}
          {downloads.state.result?.failures.map((failure) => <p key={failure.beatmapset_id} className="text-rose-200">{failure.title}：{failure.message}</p>)}
          <div className="flex flex-wrap justify-end gap-2">
            <Button size="sm" disabled={busy} onClick={() => void query.refetch()}><RefreshCw className="mr-1 size-4" />刷新</Button>
            <Button size="sm" disabled={busy || !!query.error || !query.data?.entries.length} onClick={() => sync.mutate()}>{sync.isPending ? "正在同步…" : "同步到收藏"}</Button>
            {downloads.state.busy && <Button size="sm" onClick={() => void downloads.cancel()}>取消当前下载</Button>}
            <Button size="sm" variant="primary" disabled={busy || !!query.error || downloads.state.busy || choosing || !items.length || provider === "none"} onClick={() => void downloads.start(items, { provider })}><Download className="mr-1 size-4" />下载图池 · {items.length} 个曲包</Button>
          </div>
        </footer>
      </Dialog.Content>
    </Dialog.Portal>
  </Dialog.Root>;
}
