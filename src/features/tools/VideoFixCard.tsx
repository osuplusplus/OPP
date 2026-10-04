import { useEffect, useState } from "react";
import { AlertTriangle, Film, FolderOpen, Search, Square } from "lucide-react";

import { ErrorPanel } from "../../shared/components/ErrorPanel";
import { Button, Card, SectionTitle } from "../../shared/components/ui";
import { byteSize } from "../../shared/lib/format";
import { desktopApi } from "../../shared/lib/tauri";
import type { FfmpegStatusInfo, VideoFixProgress, VideoFixResult, VideoScanResult } from "../../shared/types/osu";

const videoFixPhaseLabels: Record<VideoFixProgress["phase"], string> = {
  scan: "扫描视频文件",
  copy: "无损重封装为 FLV",
  transcode: "按原码率二次转码",
};

/** 待修复明细在卡片中最多展开的条数，其余只显示计数。 */
const visiblePending = 8;

export function VideoFixCard() {
  const [acknowledged, setAcknowledged] = useState(false);
  const [busy, setBusy] = useState<"scan" | "fix" | null>(null);
  const [progress, setProgress] = useState<VideoFixProgress | null>(null);
  const [scan, setScan] = useState<VideoScanResult | null>(null);
  const [result, setResult] = useState<VideoFixResult | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [root, setRoot] = useState<string | null>(null);
  const [ffmpeg, setFfmpeg] = useState<FfmpegStatusInfo | null>(null);
  const [error, setError] = useState<unknown>(null);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void desktopApi.onVideoFixProgress((value) => { if (!disposed) setProgress(value); }).then((dispose) => { if (disposed) dispose(); else unlisten = dispose; });
    return () => { disposed = true; unlisten?.(); };
  }, []);

  useEffect(() => {
    let disposed = false;
    void desktopApi.liveRenderGetFfmpegStatus().then((status) => { if (!disposed) setFfmpeg(status); }).catch(() => undefined);
    return () => { disposed = true; };
  }, []);

  const chooseDirectory = async () => {
    const selected = await desktopApi.chooseDirectory("选择 osu! 谱面目录（Songs）", root);
    if (!selected) return;
    setRoot(selected);
    setScan(null);
    setResult(null);
    setConfirming(false);
  };

  const runScan = async () => {
    setBusy("scan");
    setError(null);
    setResult(null);
    setConfirming(false);
    try {
      setScan(await desktopApi.scanOsuVideos(root));
    } catch (caught) {
      setError(caught);
    } finally {
      setBusy(null);
    }
  };

  const runFix = async () => {
    setBusy("fix");
    setError(null);
    setProgress(null);
    try {
      setResult(await desktopApi.fixOsuVideos(root));
      setScan(null);
      setConfirming(false);
    } catch (caught) {
      setError(caught);
    } finally {
      setBusy(null);
      setProgress(null);
    }
  };

  const ffmpegMissing = ffmpeg !== null && !ffmpeg.path;
  const pendingCount = scan?.pending_count ?? 0;

  return <Card className="p-6">
    <div className="flex items-start gap-4">
      <div className="grid size-11 shrink-0 place-items-center rounded-2xl border border-[var(--theme-primary-soft)] bg-[var(--theme-primary-muted)] text-[var(--theme-primary)]"><Film className="size-5" /></div>
      <SectionTitle title="视频闪退修复" description="osu!stable 加载部分 MP4/M4V 视频时会卡在加载界面甚至闪退。复刻社区工具 osu-video-convert 的 mp42flv 方案：先用 FFmpeg 将视频无损重封装为 FLV 容器，编码不兼容时按原码率二次转码，再把 FLV 内容写回原文件并保留原扩展名。" />
    </div>

    <p className="mt-5 text-sm leading-6 text-slate-300">扫描谱面目录（Songs）中的 .mp4／.m4v／.flv／.avi 视频，把仍是 MP4 容器的视频转换为 FLV 内容后覆盖原文件，osu!stable 即可正常加载播放。</p>

    <div className="mt-4 rounded-xl border border-amber-300/20 bg-amber-300/[0.06] p-4">
      <p className="flex items-center gap-2 text-sm font-semibold text-amber-100"><AlertTriangle className="size-4 shrink-0" />使用前请阅读风险提示</p>
      <ul className="mt-2 space-y-1.5 text-xs leading-5 text-amber-100/80">
        <li>· 修复会用转换结果覆盖 Songs 中的原始视频文件，无法撤销；重要视频请先自行备份</li>
        <li>· 编码与 FLV 不兼容的视频（H.265／VP9／AV1 等）需要按原码率二次转码：有损，且视视频大小可能耗时数分钟</li>
        <li>· 修复前请关闭 osu!stable；修复后文件内容是 FLV 容器但保留原扩展名，属预期结果</li>
        <li>· 需要 FFmpeg：优先使用设置中指定的路径，其次 PATH 与 danser 目录自带的 FFmpeg</li>
      </ul>
      <label className="mt-3 flex cursor-pointer items-center gap-2 text-xs font-medium text-amber-100">
        <input checked={acknowledged} className="size-3.5 accent-[var(--theme-primary)]" onChange={(event) => setAcknowledged(event.target.checked)} type="checkbox" />
        我已了解上述风险，确认在 osu! 谱面目录上执行
      </label>
    </div>

    {ffmpeg === null ? null : ffmpegMissing
      ? <p className="mt-4 text-xs leading-5 text-amber-200">未检测到 FFmpeg，修复无法执行：请在设置 → 常规 中指定 ffmpeg 路径，或安装 FFmpeg 后加入 PATH。</p>
      : <p className="mt-4 truncate font-mono text-xs text-slate-500" title={`${ffmpeg.path ?? ""}${ffmpeg.version ? `\n${ffmpeg.version}` : ""}`}>FFmpeg：{ffmpeg.path}</p>}

    <div className="mt-5 flex flex-wrap items-center gap-3">
      <Button disabled={busy !== null} loading={busy === "scan"} onClick={() => void runScan()}><Search className="size-4" />{scan ? "重新扫描视频" : "扫描视频"}</Button>
      <Button disabled={busy !== null} onClick={() => void chooseDirectory()} variant="secondary"><FolderOpen className="size-4" />{root ? "改选其他目录" : "选择其他目录"}</Button>
      {busy ? <Button onClick={() => void desktopApi.cancelVideoFix()} variant="secondary"><Square className="size-4" />取消</Button> : null}
    </div>
    <p className="mt-3 truncate font-mono text-xs text-slate-500" title={scan?.songs_root ?? root ?? undefined}>{scan ? `扫描目录：${scan.songs_root}` : root ? `已选择目录：${root}` : "默认使用检测到的 osu!stable Songs 目录"}</p>

    {busy && progress ? <div className="mt-4 rounded-xl border border-white/[0.08] bg-white/[0.03] p-4">
      <div className="flex items-center justify-between gap-3 text-xs text-slate-400">
        <span>{videoFixPhaseLabels[progress.phase] ?? progress.phase}</span>
        <span className="tabular-nums">{progress.total > 0 ? `${progress.processed.toLocaleString()} / ${progress.total.toLocaleString()}` : `${progress.processed.toLocaleString()} 个文件`}</span>
      </div>
      <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-white/[0.08]"><div className="h-full rounded-full bg-[var(--theme-primary)] transition-[width]" style={{ width: `${progress.percent}%` }} /></div>
      {progress.current ? <p className="mt-2 truncate font-mono text-xs text-slate-500" title={progress.current}>{progress.current}</p> : null}
    </div> : null}

    {scan ? <div className="mt-5 space-y-3">
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <div className="rounded-xl border border-white/[0.08] bg-white/[0.03] p-4"><p className="text-xs text-slate-500">视频文件</p><p className="mt-1 text-lg font-semibold tabular-nums text-white">{scan.video_count.toLocaleString()} · {byteSize(scan.video_size)}</p></div>
        <div className="rounded-xl border border-[var(--theme-primary-soft)] bg-[var(--theme-primary-muted)]/40 p-4"><p className="text-xs text-slate-500">待修复</p><p className="mt-1 text-lg font-semibold tabular-nums text-[var(--theme-primary-light)]">{scan.pending_count.toLocaleString()} · {byteSize(scan.pending_size)}</p></div>
        <div className="rounded-xl border border-white/[0.08] bg-white/[0.03] p-4"><p className="text-xs text-slate-500">已是 FLV 内容</p><p className="mt-1 text-lg font-semibold tabular-nums text-slate-300">{scan.already_flv_count.toLocaleString()}</p></div>
        <div className="rounded-xl border border-white/[0.08] bg-white/[0.03] p-4"><p className="text-xs text-slate-500">空文件／非 MP4 容器</p><p className="mt-1 text-lg font-semibold tabular-nums text-slate-300">{(scan.empty_count + scan.other_count).toLocaleString()}</p></div>
      </div>
      {scan.pending.length ? <div className="rounded-xl border border-white/[0.08] bg-white/[0.03] p-4">
        <p className="text-xs text-slate-400">待修复视频（{scan.pending_count.toLocaleString()} 个）：</p>
        <div className="mt-2 max-h-40 space-y-1 overflow-y-auto">
          {scan.pending.slice(0, visiblePending).map((entry) => <p className="truncate font-mono text-xs text-slate-500" key={entry.path} title={entry.path}>{entry.path} — {byteSize(entry.size)}</p>)}
          {scan.pending.length > visiblePending || scan.pending_truncated ? <p className="text-xs text-slate-500">…… 另有 {Math.max(scan.pending_count - visiblePending, 0).toLocaleString()} 个未列出</p> : null}
        </div>
      </div> : null}
      {scan.pending_count === 0 ? <p className="text-xs text-emerald-200">没有需要修复的视频：扫描到的文件都已是 FLV 内容、为空文件或非 MP4 容器。</p> : null}
    </div> : null}

    {scan && pendingCount > 0 && !busy ? (
      confirming ? <div className="mt-5 rounded-xl border border-rose-400/25 bg-rose-400/[0.07] p-4">
        <p className="text-sm leading-6 text-rose-100">即将修复 <span className="font-semibold">{pendingCount.toLocaleString()}</span> 个视频（共 <span className="font-semibold">{byteSize(scan.pending_size)}</span>），原文件会被转换结果覆盖且无法撤销。请确认已关闭 osu!stable。</p>
        <div className="mt-3 flex flex-wrap gap-3">
          <Button disabled={!acknowledged || ffmpegMissing} onClick={() => void runFix()} variant="danger"><Film className="size-4" />确认修复</Button>
          <Button onClick={() => setConfirming(false)} variant="secondary">取消</Button>
        </div>
      </div> : <div className="mt-5">
        <Button disabled={!acknowledged || ffmpegMissing} onClick={() => setConfirming(true)} variant="primary"><Film className="size-4" />修复 {pendingCount.toLocaleString()} 个视频</Button>
      </div>
    ) : null}

    {result ? <div className="mt-5 space-y-3">
      <p className="truncate font-mono text-xs text-slate-400" title={result.songs_root}>{result.songs_root}</p>
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <div className="rounded-xl border border-[var(--theme-primary-soft)] bg-[var(--theme-primary-muted)]/40 p-4"><p className="text-xs text-slate-500">已修复</p><p className="mt-1 text-lg font-semibold tabular-nums text-[var(--theme-primary-light)]">{result.fixed_count.toLocaleString()} · {byteSize(result.fixed_size)}</p></div>
        <div className="rounded-xl border border-white/[0.08] bg-white/[0.03] p-4"><p className="text-xs text-slate-500">无损重封装</p><p className="mt-1 text-lg font-semibold tabular-nums text-white">{result.remuxed_count.toLocaleString()}</p></div>
        <div className="rounded-xl border border-white/[0.08] bg-white/[0.03] p-4"><p className="text-xs text-slate-500">有损二次转码</p><p className="mt-1 text-lg font-semibold tabular-nums text-slate-300">{result.transcoded_count.toLocaleString()}</p></div>
        <div className="rounded-xl border border-white/[0.08] bg-white/[0.03] p-4"><p className="text-xs text-slate-500">无需处理</p><p className="mt-1 text-lg font-semibold tabular-nums text-slate-300">{result.skipped_count.toLocaleString()}</p></div>
      </div>
      {result.cancelled ? <p className="text-xs text-amber-200">任务已取消，以上为部分结果；再次修复会跳过已完成的部分。</p> : null}
      {!result.cancelled && result.fixed_count > 0 ? <p className="text-xs text-emerald-200">完成：{result.fixed_count.toLocaleString()} 个视频已替换为 FLV 内容，可启动 osu!stable 验证对应谱面。</p> : null}
      {result.transcoded_count > 0 ? <p className="text-xs text-amber-200">其中 {result.transcoded_count.toLocaleString()} 个视频因编码与 FLV 不兼容执行了有损转码，画质可能与原视频略有差异。</p> : null}
      {result.failed_count > 0 ? <div className="rounded-xl border border-white/[0.08] bg-white/[0.03] p-4">
        <p className="text-xs text-slate-400">{result.failed_count.toLocaleString()} 个视频处理失败（仅展示前 {result.failed.length} 条）：</p>
        <div className="mt-2 max-h-32 space-y-1 overflow-y-auto">
          {result.failed.map((failure) => <p className="truncate font-mono text-xs text-slate-500" key={failure.path} title={`${failure.path}：${failure.message}`}>{failure.path} — {failure.message}</p>)}
        </div>
      </div> : null}
      {!result.cancelled && result.fixed_count === 0 && result.failed_count === 0 ? <p className="text-xs text-slate-400">没有需要修复的视频。</p> : null}
    </div> : null}

    {error ? <div className="mt-4"><ErrorPanel error={error} /></div> : null}
  </Card>;
}
