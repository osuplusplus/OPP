import { useEffect, useMemo, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import {
  Ban,
  CheckCircle2,
  DownloadCloud,
  FolderOpen,
  Gauge,
  ListPlus,
  Trash2,
  X,
} from "lucide-react";

import { Badge, Button, Card } from "../../shared/components/ui";
import { desktopApi } from "../../shared/lib/tauri";
import type {
  BeatmapDownloadProvider,
  BeatmapDownloadProgress,
  BeatmapDownloadResult,
  CommandError,
  OnlineBeatmapSearchQuery,
  OnlineBeatmapset,
} from "../../shared/types/osu";
import { settingsQueryKey, useSettings } from "../settings/api";
import { resolveDefaultDownloadProvider } from "./downloadProvider";

export const beatmapDownloadDirectoryKey = "opp:beatmap-download-directory";
const suppressCompletionPromptKey = "opp:suppress-download-completion-prompt";

function formatTransfer(value: number) {
  if (!Number.isFinite(value) || value <= 0) return "计算中";
  const units = ["B/s", "KB/s", "MB/s", "GB/s"];
  let size = value;
  let index = 0;
  while (size >= 1024 && index < units.length - 1) {
    size /= 1024;
    index += 1;
  }
  return `${size.toFixed(size >= 10 ? 0 : 1)} ${units[index]}`;
}

function formatBytes(value: number) {
  return `${(value / 1024 / 1024).toFixed(1)} MB`;
}

function CompletionPreferenceDialog({ onClose }: { onClose: () => void }) {
  const queryClient = useQueryClient();
  const settings = useSettings();
  const [openAfterDownload, setOpenAfterDownload] = useState(false);
  const [dontAskAgain, setDontAskAgain] = useState(false);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    const timeout = window.setTimeout(onClose, 5000);
    return () => window.clearTimeout(timeout);
  }, [onClose]);

  const save = async () => {
    if (dontAskAgain) localStorage.setItem(suppressCompletionPromptKey, "true");
    if (openAfterDownload && settings.data) {
      setSaving(true);
      try {
        const next = await desktopApi.updateSettings({
          ...settings.data,
          open_downloaded_beatmaps_after_download: true,
        });
        queryClient.setQueryData(settingsQueryKey, next);
      } finally {
        setSaving(false);
      }
    }
    onClose();
  };

  return (
    <div aria-labelledby="download-preference-title" aria-modal="true" className="fixed inset-0 z-[120] grid place-items-center bg-black/55 p-5" role="dialog">
      <div className="w-full max-w-md rounded-2xl border border-white/[0.12] bg-[var(--surface-panel)] p-6 shadow-2xl">
        <h2 className="text-lg font-semibold text-white" id="download-preference-title">下载完成</h2>
        <p className="mt-2 text-sm leading-6 text-slate-400">是否为之后下载的谱面启用自动打开？你也可以以后在设置中调整。</p>
        <label className="mt-5 flex cursor-pointer items-center gap-3 text-sm text-slate-200"><input checked={openAfterDownload} className="size-4 accent-[var(--theme-primary)]" onChange={(event) => setOpenAfterDownload(event.target.checked)} type="checkbox" />下载完成后自动打开谱面</label>
        <label className="mt-3 flex cursor-pointer items-center gap-3 text-sm text-slate-400"><input checked={dontAskAgain} className="size-4 accent-[var(--theme-primary)]" onChange={(event) => setDontAskAgain(event.target.checked)} type="checkbox" />不再提示</label>
        <div className="mt-6 flex justify-end gap-3"><Button onClick={onClose} variant="ghost">暂不设置</Button><Button loading={saving} onClick={() => void save()} variant="primary">保存</Button></div>
      </div>
    </div>
  );
}

export function BeatmapDownloadPanel({
  availableTotal,
  externalDownloadActive = false,
  query,
  queue,
  onClear,
  onRemove,
  onReplace,
}: {
  availableTotal: number | null;
  externalDownloadActive?: boolean;
  query: OnlineBeatmapSearchQuery;
  queue: OnlineBeatmapset[];
  onClear: () => void;
  onRemove: (beatmapsetId: number) => void;
  onReplace: (items: OnlineBeatmapset[]) => void;
}) {
  const [destination, setDestination] = useState(() => localStorage.getItem(beatmapDownloadDirectoryKey) ?? "");
  const settings = useSettings();
  const queryClient = useQueryClient();
  const [collectLimit, setCollectLimit] = useState(100);
  const [providerOverride, setProviderOverride] = useState<
    BeatmapDownloadProvider | "none" | null
  >(null);
  const [collecting, setCollecting] = useState(false);
  const [downloading, setDownloading] = useState(false);
  const [overwrite, setOverwrite] = useState(false);
  const [progress, setProgress] = useState<BeatmapDownloadProgress | null>(null);
  const [result, setResult] = useState<BeatmapDownloadResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showCompletionPreference, setShowCompletionPreference] = useState(false);
  const effectiveDestination = destination || settings.data?.beatmap_download_directory || "";
  const provider = providerOverride ?? resolveDefaultDownloadProvider(settings.data);

  useEffect(() => {
    let dispose: () => void = () => undefined;
    void desktopApi.onBeatmapDownloadProgress((next) => setProgress(next)).then((unlisten) => { dispose = unlisten; });
    return () => dispose();
  }, []);

  const percent = useMemo(
    () => {
      if (!progress?.total) return 0;
      const currentFileProgress = progress.total_bytes
        ? Math.min(1, (progress.downloaded_bytes ?? 0) / progress.total_bytes)
        : 0;
      return Math.min(100, ((progress.processed + currentFileProgress) / progress.total) * 100);
    },
    [progress],
  );

  const saveDestination = async (selected: string) => {
    setDestination(selected);
    localStorage.setItem(beatmapDownloadDirectoryKey, selected);
    if (settings.data) {
      const next = await desktopApi.updateSettings({ ...settings.data, beatmap_download_directory: selected });
      queryClient.setQueryData(settingsQueryKey, next);
    }
  };

  const chooseDirectory = async () => {
    try {
      const selected = await desktopApi.chooseBeatmapDownloadDirectory(effectiveDestination || null);
      if (selected) await saveDestination(selected);
    } catch (caught) {
      setError((caught as CommandError).message ?? String(caught));
    }
  };

  const collectMatches = async () => {
    setCollecting(true);
    setError(null);
    setResult(null);
    try {
      const collected = await desktopApi.collectOnlineBeatmapsets({ ...query, cursor_string: null }, collectLimit);
      onReplace(collected.items.filter((item) => !item.availability?.download_disabled));
      if (collected.truncated) setError(`结果超过 ${collectLimit} 条，已加入前 ${collectLimit} 条。`);
    } catch (caught) {
      setError((caught as CommandError).message ?? String(caught));
    } finally {
      setCollecting(false);
    }
  };

  const startDownload = async () => {
    let target = effectiveDestination;
    if (!target) {
      target = await desktopApi.chooseBeatmapDownloadDirectory(null) ?? "";
      if (!target) return;
      await saveDestination(target);
    }
    setDownloading(true);
    setError(null);
    setResult(null);
    setProgress(null);
    try {
      const completed = await desktopApi.downloadOnlineBeatmapsets({
        destination: target,
        provider,
        overwrite,
        include_video: settings.data?.include_video_in_beatmap_downloads ?? true,
        items: queue.map((item) => ({ beatmapset_id: item.id, artist: item.artist, title: item.title })),
      });
      setResult(completed);
      if (completed.completed > 0 && !settings.data?.open_downloaded_beatmaps_after_download && localStorage.getItem(suppressCompletionPromptKey) !== "true") setShowCompletionPreference(true);
    } catch (caught) {
      setError((caught as CommandError).message ?? String(caught));
    } finally {
      setDownloading(false);
    }
  };

  const busy = downloading || externalDownloadActive;

  return (
    <>
      <Card className="opp-download-panel opp-online-panel sticky top-[120px] overflow-hidden rounded-[11px] border border-[var(--line-subtle)] bg-[color-mix(in_srgb,var(--surface-panel)_94%,transparent)] shadow-[0_14px_34px_rgba(0,0,0,0.08)]" data-page-guide-online-download="true" unstyled>
        <div className="border-b border-white/[0.08] px-5 py-4"><div className="flex items-center justify-between"><h2 className="text-base font-semibold text-white">批量下载队列</h2><Badge tone={queue.length ? "pink" : "neutral"}>{queue.length}</Badge></div></div>
        <div className="space-y-5 p-5">
          <button className="flex w-full items-center gap-3 rounded-xl border border-white/[0.09] bg-black/15 px-3 py-3 text-left text-base text-slate-300 transition hover:border-white/20" disabled={busy || collecting} onClick={() => void chooseDirectory()} type="button"><FolderOpen className="size-5 shrink-0 text-cyan-200" /><span className="min-w-0 flex-1 truncate">{effectiveDestination || "选择保存目录"}</span></button>
          <label className="block"><span className="mb-2 block text-sm font-medium text-slate-300">下载适配器</span><select className="w-full rounded-xl border border-white/[0.09] bg-[#0b101b] px-3 py-3 text-sm text-slate-200 outline-none focus:border-cyan-300/45" disabled={busy || collecting} onChange={(event) => setProviderOverride(event.target.value as BeatmapDownloadProvider | "none")} value={provider}><option value="sayobot">小夜（Sayobot，推荐）</option><option value="hinai">Hinai Mirror（多源回退）</option><option value="catboy">Catboy</option><option value="nerinyan">Nerinyan</option><option value="none">不使用镜像</option></select></label>
          <div className="rounded-2xl border border-cyan-300/10 bg-cyan-300/[0.035] p-4"><div className="flex items-center justify-between gap-3"><div><p className="text-sm font-medium text-slate-200">按当前筛选加入</p><p className="mt-1 text-sm text-slate-500">官网匹配 {availableTotal ?? "—"} 条</p></div><select className="rounded-lg border border-white/[0.09] bg-[#0b101b] px-2 py-2 text-sm text-slate-300" disabled={collecting || busy} onChange={(event) => setCollectLimit(Number(event.target.value))} value={collectLimit}>{[50, 100, 250, 500].map((limit) => <option key={limit} value={limit}>{limit} 条</option>)}</select></div><Button className="mt-3 w-full" disabled={busy} loading={collecting} onClick={() => void collectMatches()} size="sm"><ListPlus className="size-4" />加入当前结果</Button></div>
          <div className="max-h-72 space-y-1 overflow-y-auto pr-1">{queue.length ? queue.slice(0, 100).map((item, index) => <div className="group flex items-center gap-3 rounded-xl px-2 py-2.5 hover:bg-white/[0.04]" key={item.id}><span className="w-6 shrink-0 text-center font-mono text-sm text-slate-600">{index + 1}</span><div className="min-w-0 flex-1"><p className="truncate text-sm text-slate-300">{item.title}</p><p className="truncate text-xs text-slate-600">{item.artist} · {item.creator}</p></div><button aria-label="移除谱面" className="grid size-8 place-items-center rounded-lg text-slate-600 opacity-0 transition hover:bg-white/[0.08] hover:text-white group-hover:opacity-100" disabled={busy} onClick={() => onRemove(item.id)} type="button"><X className="size-4" /></button></div>) : <div className="rounded-xl border border-dashed border-white/[0.1] px-4 py-10 text-center text-sm leading-6 text-slate-600">从结果卡片选择谱面，或按当前筛选批量加入。</div>}</div>
          {progress ? <div className="rounded-xl border border-white/[0.08] bg-black/15 p-4"><div className="flex items-center justify-between gap-3 text-sm"><span className="max-w-[180px] truncate text-slate-400">{progress.current_title ?? progress.message ?? "正在下载"}</span><span className="font-mono text-[var(--theme-primary)]">{progress.processed}/{progress.total}</span></div><div className="mt-3 h-2 overflow-hidden rounded-full bg-white/[0.08]"><div className="h-full rounded-full bg-[var(--theme-primary)] transition-all" style={{ width: `${percent}%` }} /></div><div className="mt-3 flex items-center justify-between gap-2 text-xs text-slate-500"><span className="inline-flex items-center gap-1.5"><Gauge className="size-3.5" />{progress.phase === "finished" ? "下载完成" : progress.phase === "cancelled" ? "已取消" : "下载中"}</span><span className="font-mono text-slate-400">{formatBytes(progress.downloaded_bytes ?? 0)}{progress.total_bytes ? ` / ${formatBytes(progress.total_bytes)}` : ""}</span><span className="font-mono text-emerald-200">{formatTransfer(progress.bytes_per_second ?? 0)}</span></div></div> : null}
          {result ? <div className="flex items-start gap-3 rounded-xl border border-emerald-300/10 bg-emerald-300/[0.05] p-4 text-sm text-emerald-100"><CheckCircle2 className="mt-0.5 size-5 shrink-0" />完成 {result.completed}，跳过 {result.skipped}，失败 {result.failed}</div> : null}
          {error ? <div className="rounded-xl border border-amber-300/10 bg-amber-300/[0.05] p-4 text-sm leading-6 text-amber-100">{error}</div> : null}
          <label className="flex items-center gap-3 text-sm text-slate-500"><input checked={overwrite} disabled={busy} onChange={(event) => setOverwrite(event.target.checked)} type="checkbox" />覆盖同名 .osz 文件</label>
          <div className="flex gap-3">{downloading ? <Button className="flex-1" onClick={() => void desktopApi.cancelOnlineBeatmapDownload()} variant="danger"><Ban className="size-4" />取消下载</Button> : <Button className="flex-1" disabled={!queue.length || provider === "none" || collecting || externalDownloadActive} onClick={() => void startDownload()} variant="primary"><DownloadCloud className="size-4" />开始下载</Button>}<Button aria-label="清空队列" disabled={!queue.length || busy} onClick={onClear} size="icon" variant="ghost"><Trash2 className="size-5" /></Button></div>
          <p className="text-sm leading-6 text-slate-600">谱面信息和筛选来自官网；镜像站仅提供文件资源。</p>
        </div>
      </Card>
      {showCompletionPreference ? <CompletionPreferenceDialog onClose={() => setShowCompletionPreference(false)} /> : null}
    </>
  );
}
