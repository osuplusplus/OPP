import { useEffect, useRef, useState } from "react";
import { AlertTriangle, ArrowUp, CheckCircle2, ChevronDown, ChevronUp, FolderOpen, Loader2, Play, Settings2, X } from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import { Outlet, useLocation, useNavigate } from "react-router-dom";
import type { AppSettings, BeatmapDownloadProgress, CollectionTaskProgress, CommandError, NewReplaysDetected, Ruleset } from "../shared/types/osu";
import { authQueryKey } from "../features/auth/api";
import { useOwnProfile } from "../features/profile/api";
import { useMode } from "./ModeContext";
import { Sidebar } from "./Sidebar";
import { GlobalContextBar } from "./GlobalContextBar";
import { desktopApi } from "../shared/lib/tauri";
import type { GameSessionSummary } from "../shared/types/osu";
import { dateTime, fullNumber, percent } from "../shared/lib/format";
import { Badge, Button, Card, DataLine } from "../shared/components/ui";
import { CollectionAddDialog } from "../features/collections/CollectionAddDialog";
import { requestCollectionTaskCancellation, subscribeCollectionTask, updateCollectionTask, type CollectionTaskStatus } from "../features/collections/taskStatus";
import { settingsQueryKey, useSettings } from "../features/settings/api";
import { OnboardingTour } from "../features/onboarding/OnboardingTour";
import {
  CURRENT_ONBOARDING_VERSION,
  needsOnboarding,
} from "../features/onboarding/tourContent";
import {
  getPageGuide,
  needsPageOnboarding,
  type PageGuide,
} from "../features/onboarding/pageTourContent";
import { START_ONBOARDING_EVENT, START_PAGE_ONBOARDING_EVENT } from "../shared/lib/onboardingEvents";
import { UpdateCenter } from "../features/updates/UpdateCenter";
import { usesBeatmapWorkspaceLayout } from "./workspaceLayout";

const validRulesets: Ruleset[] = ["osu", "taiko", "fruits", "mania"];

function formatTransfer(value: number) {
  if (!Number.isFinite(value) || value <= 0) return "计算中";
  const units = ["B/s", "KB/s", "MB/s", "GB/s"];
  let size = value; let index = 0;
  while (size >= 1024 && index < units.length - 1) { size /= 1024; index += 1; }
  return `${size.toFixed(size >= 10 ? 0 : 1)} ${units[index]}`;
}

function downloadProgressPercent(progress: BeatmapDownloadProgress) {
  if (!progress.total) return 0;
  const currentFileProgress = progress.total_bytes
    ? Math.min(1, (progress.downloaded_bytes ?? 0) / progress.total_bytes)
    : 0;
  return Math.min(100, ((progress.processed + currentFileProgress) / progress.total) * 100);
}

function formatDownloadedBytes(progress: BeatmapDownloadProgress) {
  const downloaded = progress.downloaded_bytes ?? 0;
  if (!downloaded) return progress.message ?? "等待连接";
  const current = `${(downloaded / 1024 / 1024).toFixed(1)} MB`;
  return progress.total_bytes
    ? `${current} / ${(progress.total_bytes / 1024 / 1024).toFixed(1)} MB`
    : current;
}

function DownloadToast() {
  const [progress, setProgress] = useState<BeatmapDownloadProgress | null>(null);
  const [displaySpeed, setDisplaySpeed] = useState<number | null>(null);
  const [cancelling, setCancelling] = useState(false);
  useEffect(() => {
    let dispose: (() => void) | undefined;
    let timer: number | undefined;
    void desktopApi.onBeatmapDownloadProgress((next) => {
      window.clearTimeout(timer);
      setProgress(next);
      if (next.phase === "started") {
        setDisplaySpeed(null);
        setCancelling(false);
      } else if (next.phase === "downloading" && Number.isFinite(next.bytes_per_second) && (next.bytes_per_second ?? 0) > 0) {
        setDisplaySpeed((previous) => previous === null
          ? next.bytes_per_second ?? null
          : previous * 0.72 + (next.bytes_per_second ?? 0) * 0.28);
      }
      if (next.phase === "finished" || next.phase === "cancelled") timer = window.setTimeout(() => {
        setCancelling(false);
        setProgress(null);
        setDisplaySpeed(null);
      }, 5_000);
    }).then((unlisten) => { dispose = unlisten; });
    return () => { window.clearTimeout(timer); dispose?.(); };
  }, []);
  if (!progress) return null;
  const completed = progress.phase === "finished" || progress.phase === "cancelled";
  const percent = downloadProgressPercent(progress);
  if (progress.phase === "cancelled") return (
    <div aria-live="polite" className="fixed bottom-6 right-6 z-[180] w-[340px] rounded-2xl border border-amber-300/20 bg-[#0b101b]/95 p-4 shadow-2xl backdrop-blur">
      <p className="text-sm font-semibold text-white">下载已取消</p>
      <p className="mt-1 text-xs text-slate-400">{progress.message ?? "未完成的谱面不会继续下载。"}</p>
    </div>
  );
  const cancelDownload = async () => {
    setCancelling(true);
    try {
      await desktopApi.cancelOnlineBeatmapDownload();
    } catch {
      setCancelling(false);
    }
  };
  if (!completed) return (
    <div aria-live="polite" className="fixed bottom-6 right-6 z-[180] w-[340px] rounded-2xl border border-cyan-300/20 bg-[#0b101b]/95 p-4 shadow-2xl backdrop-blur">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="text-sm font-semibold text-white">{cancelling ? "正在取消下载" : "正在下载谱面"}</p>
          <p className="mt-1 truncate text-xs text-slate-400">{progress.current_title ?? progress.message ?? "准备下载"}</p>
        </div>
        <span className="shrink-0 font-mono text-xs text-cyan-200">{progress.processed}/{progress.total}</span>
      </div>
      <div className="mt-3 h-1.5 overflow-hidden rounded-full bg-white/[0.08]"><div className="h-full rounded-full bg-[var(--theme-primary)] transition-[width]" style={{ width: `${percent}%` }} /></div>
      <div className="mt-2 flex items-center justify-between gap-3 text-xs">
        <span className="min-w-0 flex-1 truncate text-slate-500">{formatDownloadedBytes(progress)}</span>
        <strong className="shrink-0 font-mono text-emerald-200">{formatTransfer(displaySpeed ?? progress.bytes_per_second ?? 0)}</strong>
        <Button disabled={cancelling} onClick={() => void cancelDownload()} size="sm" variant="ghost"><X className="size-3.5" />{cancelling ? "取消中" : "取消下载"}</Button>
      </div>
    </div>
  );
  return <div aria-live="polite" className="fixed bottom-6 right-6 z-[180] w-[340px] rounded-2xl border border-cyan-300/20 bg-[#0b101b]/95 p-4 shadow-2xl backdrop-blur"><div className="flex items-start justify-between gap-3"><div className="min-w-0"><p className="text-sm font-semibold text-white">{completed ? "下载完成" : "正在下载谱面"}</p><p className="mt-1 truncate text-xs text-slate-400">{progress.current_title ?? progress.message ?? "准备下载"}</p></div><span className="shrink-0 font-mono text-xs text-cyan-200">{progress.processed}/{progress.total}</span></div><div className="mt-3 h-1.5 overflow-hidden rounded-full bg-white/[0.08]"><div className="h-full rounded-full bg-[var(--theme-primary)] transition-[width]" style={{ width: `${percent}%` }} /></div><div className="mt-2 flex justify-between gap-3 text-xs"><span className="truncate text-slate-500">{formatDownloadedBytes(progress)}</span><strong className="shrink-0 font-mono text-emerald-200">{formatTransfer(progress.bytes_per_second ?? 0)}</strong></div></div>;
}

function CollectionTaskToast() {
  const [status, setStatus] = useState<CollectionTaskStatus | null>(null);
  const [collapsed, setCollapsed] = useState(false);
  const active = useRef(false);
  const [cancelling, setCancelling] = useState(false);

  useEffect(() => subscribeCollectionTask((next) => {
    active.current = !["completed", "failed", "cancelled"].includes(next.phase);
    if (next.phase !== "cancelled") setCancelling(false);
    setStatus(next);
  }), []);

  useEffect(() => {
    let disposeCollection: (() => void) | undefined;
    let disposeDownload: (() => void) | undefined;
    void desktopApi.onCollectionTaskProgress((progress: CollectionTaskProgress) => {
      if (!active.current) return;
      updateCollectionTask({
        phase: progress.phase,
        message: progress.message,
        processed: progress.processed,
        total: progress.total,
      });
    }).then((unlisten) => { disposeCollection = unlisten; });
    void desktopApi.onBeatmapDownloadProgress((progress) => {
      if (!active.current) return;
      if (progress.phase === "cancelled") {
        updateCollectionTask({ phase: "cancelled", message: "缺失曲包下载已取消" });
        return;
      }
      updateCollectionTask({
        phase: "downloading",
        message: progress.current_title ?? progress.message ?? "正在下载缺失曲包",
        processed: progress.processed,
        total: progress.total,
      });
    }).then((unlisten) => { disposeDownload = unlisten; });
    return () => { disposeCollection?.(); disposeDownload?.(); };
  }, []);

  if (!status) return null;
  const terminal = ["completed", "failed", "cancelled"].includes(status.phase);
  const percent = status.total ? Math.min(100, (status.processed / status.total) * 100) : 0;
  const labels: Record<CollectionTaskStatus["phase"], string> = {
    checking: "解析缺失谱面",
    downloading: "下载曲包",
    installing: "读取曲包并补齐 MD5",
    writing: "写回游戏收藏夹",
    opening: "调用游戏导入曲包",
    completed: "收藏夹同步完成",
    failed: "收藏夹同步失败",
    cancelled: "收藏夹同步已取消",
  };

  const cancelTask = async () => {
    setCancelling(true);
    requestCollectionTaskCancellation();
    try {
      await desktopApi.cancelCollectionTask();
    } catch {
      await desktopApi.cancelOnlineBeatmapDownload().catch(() => undefined);
    }
  };

  if (collapsed) return <button className="fixed right-6 top-[124px] z-[210] flex items-center gap-2 rounded-xl border border-cyan-300/20 bg-[#0b101b]/95 px-4 py-3 text-sm text-cyan-100 shadow-2xl backdrop-blur" onClick={() => setCollapsed(false)} type="button">{terminal ? status.phase === "completed" ? <CheckCircle2 className="size-4 text-emerald-300" /> : <AlertTriangle className="size-4 text-amber-300" /> : <Loader2 className="size-4 animate-spin" />}<span>{labels[status.phase]}</span><ChevronDown className="size-4 text-slate-500" /></button>;

  return <section aria-live="polite" className="fixed right-6 top-[124px] z-[210] w-[390px] overflow-hidden rounded-2xl border border-cyan-300/20 bg-[#0b101b]/95 shadow-2xl backdrop-blur"><div className="flex items-start gap-3 border-b border-white/[0.08] p-4"><span className="mt-0.5">{terminal ? status.phase === "completed" ? <CheckCircle2 className="size-5 text-emerald-300" /> : <AlertTriangle className="size-5 text-amber-300" /> : <Loader2 className="size-5 animate-spin text-cyan-300" />}</span><div className="min-w-0 flex-1"><h2 className="text-sm font-semibold text-white">{labels[status.phase]}</h2><p className="mt-1 text-xs text-slate-500">后台执行中，可以自由切换到其他页面</p></div><button aria-label="最小化同步进度" className="text-slate-500 hover:text-white" onClick={() => setCollapsed(true)} type="button"><ChevronUp className="size-4" /></button>{terminal ? <button aria-label="关闭同步进度" className="text-slate-500 hover:text-white" onClick={() => setStatus(null)} type="button"><X className="size-4" /></button> : null}</div><div className="space-y-3 p-4"><p className="text-sm leading-5 text-slate-300">{status.message}</p>{status.total ? <><div className="h-2 overflow-hidden rounded-full bg-white/[0.08]"><div className={`h-full rounded-full transition-[width] ${status.phase === "failed" ? "bg-rose-400" : "bg-[var(--theme-primary)]"}`} style={{ width: `${percent}%` }} /></div><div className="flex justify-between font-mono text-xs text-slate-500"><span>{status.processed}/{status.total}</span><span>{percent.toFixed(0)}%</span></div></> : null}{status.errors.length ? <div className="max-h-36 overflow-y-auto rounded-xl border border-rose-300/15 bg-rose-300/[0.05] p-3"><p className="mb-2 text-xs font-semibold text-rose-200">错误信息</p>{status.errors.map((error, index) => <p className="mt-1 text-xs leading-5 text-rose-100/80" key={`${error}-${index}`}>{error}</p>)}</div> : null}{!terminal ? <Button disabled={cancelling} onClick={() => void cancelTask()} size="sm" variant="ghost"><X className="size-3.5" />{cancelling ? "正在取消…" : "取消任务"}</Button> : null}</div></section>;
}

function DownloadCompletedPlaylist() {
  const [files, setFiles] = useState<string[]>([]);
  const [destination, setDestination] = useState<string | null>(null);
  const [noticeVisible, setNoticeVisible] = useState(false);

  useEffect(() => {
    let dispose: (() => void) | undefined;
    let timer: number | undefined;
    void desktopApi.onBeatmapDownloadProgress((next) => {
      if (next.phase !== "finished" || !next.completed_paths?.length) return;
      window.clearTimeout(timer);
      setFiles(next.completed_paths);
      setDestination(next.destination ?? null);
      setNoticeVisible(true);
      timer = window.setTimeout(() => {
        setNoticeVisible(false);
        setFiles([]);
        setDestination(null);
      }, 5_000);
    }).then((unlisten) => { dispose = unlisten; });
    return () => { window.clearTimeout(timer); dispose?.(); };
  }, []);

  if (!files.length) return null;
  const openFirst = () => void desktopApi.openDownloadedPath(files[0]);
  return <>
    {noticeVisible ? <button aria-label="打开已下载谱面" className="fixed right-6 top-6 z-[185] w-[320px] rounded-lg border border-emerald-300/25 bg-[var(--surface-panel)] p-4 text-left shadow-xl" onDoubleClick={openFirst} type="button">
      <p className="text-sm font-semibold text-emerald-300">下载完成</p>
      <p className="mt-1 truncate text-xs text-slate-400">双击打开第一个已下载文件</p>
    </button> : null}
    <section aria-label="已下载文件" className="fixed bottom-6 right-6 z-[181] w-[340px] overflow-hidden rounded-lg border border-white/10 bg-[var(--surface-panel)] shadow-xl">
      <div className="flex items-center justify-between gap-3 border-b border-white/[0.08] px-4 py-3">
        <p className="text-sm font-semibold text-white">已下载文件</p>
        {destination ? <Button aria-label="打开下载位置" onClick={() => void desktopApi.openDownloadedPath(destination)} size="icon" title="打开下载位置" variant="ghost"><FolderOpen className="size-4" /></Button> : null}
      </div>
      <div className="max-h-44 overflow-y-auto p-2">
        {files.map((file) => <button className="flex w-full items-center gap-2 rounded-md px-2 py-2 text-left text-xs text-slate-300 hover:bg-white/[0.06]" key={file} onDoubleClick={() => void desktopApi.openDownloadedPath(file)} title={file} type="button">
          <Play className="size-3.5 shrink-0 text-[var(--theme-primary)]" />
          <span className="truncate">{file.split(/[\\/]/).pop()}</span>
        </button>)}
      </div>
    </section>
  </>;
}

export function GameCompletionOverlay({ session, discovery, settings, onClose, onNavigate }: { session: GameSessionSummary | null; discovery: NewReplaysDetected | null; settings: AppSettings | undefined; onClose: () => void; onNavigate: (path: string) => void }) {
  const [selected, setSelected] = useState<string[]>(() => settings?.auto_export_new_replays_with_danser
    ? discovery?.replays.filter((item) => item.renderable).map((item) => item.path) ?? []
    : []);
  const [danserReady, setDanserReady] = useState(false);
  const [busy, setBusy] = useState(false);
  const [renderError, setRenderError] = useState<string | null>(null);
  const end = session?.end;
  useEffect(() => { void desktopApi.getDanserStatus().then((status) => setDanserReady(status.available && status.ffmpeg_available)).catch(() => setDanserReady(false)); }, []);
  if (!end && !discovery) return null;
  const renderablePaths = discovery?.replays.filter((item) => item.renderable).map((item) => item.path) ?? [];
  const change = (before: number | null, after: number | null) => before === null || after === null ? "—" : `${after - before >= 0 ? "+" : ""}${(after - before).toFixed(2)}`;
  const integerChange = (before: number | null, after: number | null) => before === null || after === null ? "—" : `${after - before >= 0 ? "+" : ""}${fullNumber(after - before)}`;
  const enqueueSelected = async () => {
    if (!discovery || !settings?.danser_render_preferences || !selected.length) return;
    setBusy(true); setRenderError(null);
    try { await desktopApi.enqueueDanserRenders({ client: discovery.client, replay_paths: selected, preferences: settings.danser_render_preferences }); onClose(); onNavigate("/local/media/render"); }
    catch (error) { setRenderError((error as { message?: string }).message ?? String(error)); }
    finally { setBusy(false); }
  };
  return <div className="fixed inset-0 z-[100] grid place-items-center overflow-y-auto bg-black/65 p-6 backdrop-blur-sm"><Card className="my-6 w-full max-w-2xl p-6 shadow-2xl"><div className="flex items-start justify-between gap-4"><div><Badge tone="success">游戏已结束</Badge><h2 className="mt-3 text-2xl font-semibold text-white">本次游戏总结</h2><p className="mt-1 text-sm text-slate-500">{dateTime(session?.started_at ?? discovery?.started_at ?? null)} → {dateTime(session?.ended_at ?? discovery?.detected_at ?? null)}</p></div><Button onClick={onClose} size="sm">关闭</Button></div>
    {end && session ? <div className="mt-5"><DataLine label="PP" value={`${end.pp?.toFixed(2) ?? "—"} (${change(session.start.pp, end.pp)})`} /><DataLine label="计分成绩" value={`${fullNumber(end.ranked_score)} (${integerChange(session.start.ranked_score, end.ranked_score)})`} /><DataLine label="准确率" value={`${percent(end.hit_accuracy)} (${change(session.start.hit_accuracy, end.hit_accuracy)}%)`} /><DataLine label="总命中次数" value={`${fullNumber(end.total_hits)} (${integerChange(session.start.total_hits, end.total_hits)})`} /><DataLine label="总分" value={`${fullNumber(end.total_score)} (${integerChange(session.start.total_score, end.total_score)})`} /></div> : null}
    {discovery ? <section className="mt-6 border-t border-white/[0.08] pt-5"><div className="flex items-start justify-between gap-3"><div><h3 className="font-semibold text-white">发现 {discovery.replays.length} 个新回放</h3><p className="mt-1 text-xs text-slate-500">勾选本次要处理的回放并加入 Danser 队列；任务不会立即开始渲染。</p></div><Badge tone="cyan">{discovery.client}</Badge></div><div className="mt-3 flex items-center justify-between gap-3"><span className="text-xs text-slate-500">已选 {selected.length} / {renderablePaths.length}</span><div className="flex gap-2"><Button disabled={busy || !renderablePaths.length} onClick={() => setSelected(renderablePaths)} size="sm" variant="ghost">全选可渲染</Button><Button disabled={busy || !selected.length} onClick={() => setSelected([])} size="sm" variant="ghost">清空</Button></div></div><div className="mt-3 max-h-64 space-y-2 overflow-y-auto">{discovery.replays.map((item) => <label className={`flex items-start gap-3 rounded-xl border p-3 ${item.renderable ? "border-white/[0.08] bg-black/15" : "border-amber-300/15 bg-amber-300/[0.04]"}`} key={item.path}><input aria-label={`选择 ${item.beatmap_title ?? item.file_name}`} checked={selected.includes(item.path)} className="mt-1 accent-cyan-300" disabled={!item.renderable || busy} onChange={(event) => setSelected((current) => event.target.checked ? [...current, item.path] : current.filter((path) => path !== item.path))} type="checkbox" /><span className="min-w-0"><span className="block truncate text-sm font-medium text-slate-200">{item.beatmap_title ?? item.file_name}</span><span className="mt-1 block text-xs text-slate-500">{item.renderable ? item.username ?? "可使用 Danser 渲染" : item.reason}</span></span></label>)}</div>{renderError ? <p className="mt-3 text-sm text-rose-200">{renderError}</p> : null}<div className="mt-4 flex justify-end gap-2">{!danserReady || !settings?.replay_export_directory || !settings.danser_render_preferences ? <Button onClick={() => { onClose(); onNavigate("/settings"); }}><Settings2 className="size-4" />配置 Danser</Button> : <Button disabled={!selected.length} loading={busy} onClick={() => void enqueueSelected()} variant="primary"><Play className="size-4" />加入渲染队列</Button>}</div></section> : null}
  </Card></div>;
}

function TosuLaunchPrompt({ settings, onClose }: { settings: AppSettings; onClose: () => void }) {
  const [autoLaunch, setAutoLaunch] = useState(settings.launch_tosu_on_obs_detect ?? false);
  const [dontAsk, setDontAsk] = useState(settings.suppress_tosu_launch_prompt ?? false);
  const [busy, setBusy] = useState(false);
  const start = async () => { setBusy(true); try { await desktopApi.updateSettings({ ...settings, launch_tosu_on_obs_detect: autoLaunch, suppress_tosu_launch_prompt: dontAsk }); await desktopApi.startTosu(); onClose(); } finally { setBusy(false); } };
  return <div className="fixed inset-0 z-[220] grid place-items-center bg-black/60 p-6 backdrop-blur-sm"><Card className="w-full max-w-md p-6 shadow-2xl"><h2 className="text-lg font-semibold text-white">启动 tosu</h2><p className="mt-2 text-sm leading-6 text-slate-400">检测到 OBS 已启动。Tosu 就绪后会刷新所选场景中的浏览器源。</p><label className="mt-5 flex items-center gap-3 text-sm text-slate-200"><input checked={autoLaunch} onChange={(event) => setAutoLaunch(event.target.checked)} type="checkbox" />每次检测到 OBS 启动时自动打开 tosu</label><label className="mt-3 flex items-center gap-3 text-sm text-slate-200"><input checked={dontAsk} onChange={(event) => setDontAsk(event.target.checked)} type="checkbox" />不再提示</label><div className="mt-6 flex justify-end gap-2"><Button disabled={busy} onClick={onClose} variant="ghost">取消</Button><Button loading={busy} onClick={() => void start()}>启动 tosu</Button></div></Card></div>;
}

export function AppShell() {
  const { ruleset, setRuleset, hasRulesetPreference } = useMode();
  const profileQuery = useOwnProfile(ruleset);
  const settingsQuery = useSettings();
  const location = useLocation();
  const refinedWorkspace = !usesBeatmapWorkspaceLayout(location.pathname);
  const navigate = useNavigate();
  const initializedMode = useRef(hasRulesetPreference);
  const onboardingChecked = useRef(false);
  const checkedPageGuides = useRef(new Set<string>());
  const queryClient = useQueryClient();
  const [showBackToTop, setShowBackToTop] = useState(false);
  const [completedSession, setCompletedSession] = useState<GameSessionSummary | null>(null);
  const [newReplays, setNewReplays] = useState<NewReplaysDetected | null>(null);
  const [dismissedSession, setDismissedSession] = useState<string | null>(null);
  const [tosuPromptSettings, setTosuPromptSettings] = useState<AppSettings | null>(null);
  const [onboardingOpen, setOnboardingOpen] = useState(false);
  const [overallOnboardingReady, setOverallOnboardingReady] = useState(false);
  const [pageGuide, setPageGuide] = useState<PageGuide | null>(null);
  const analysisEnabled = true;

  useEffect(() => {
    let off: () => void = () => undefined;
    void desktopApi.onNewReplaysDetected(setNewReplays).then((unlisten) => { off = unlisten; });
    return () => off();
  }, []);

  useEffect(() => {
    const settings = settingsQuery.data;
    if (!settings || onboardingChecked.current) return;
    onboardingChecked.current = true;
    if (!needsOnboarding(settings.onboarding_version)) {
      const readyTimer = window.setTimeout(() => setOverallOnboardingReady(true), 0);
      return () => window.clearTimeout(readyTimer);
    }

    const openTimer = window.setTimeout(() => {
      setOnboardingOpen(true);
      void desktopApi.markOnboardingSeen(CURRENT_ONBOARDING_VERSION).then((saved) => {
        queryClient.setQueryData(settingsQueryKey, saved);
      }).catch(() => undefined);
    }, 0);
    return () => window.clearTimeout(openTimer);
  }, [queryClient, settingsQuery.data]);

  useEffect(() => {
    const startOnboarding = () => {
      setPageGuide(null);
      setOverallOnboardingReady(false);
      setOnboardingOpen(true);
    };
    window.addEventListener(START_ONBOARDING_EVENT, startOnboarding);
    return () => window.removeEventListener(START_ONBOARDING_EVENT, startOnboarding);
  }, []);

  useEffect(() => {
    if (!overallOnboardingReady || onboardingOpen || pageGuide) return;
    const guide = getPageGuide(location.pathname);
    const settings = settingsQuery.data;
    if (!guide || !settings || checkedPageGuides.current.has(guide.id)) return;
    checkedPageGuides.current.add(guide.id);
    if (!needsPageOnboarding(settings.page_onboarding_versions?.[guide.id], guide)) return;

    const openTimer = window.setTimeout(() => {
      setPageGuide(guide);
      void desktopApi.markPageOnboardingSeen(guide.id, guide.version).then((saved) => {
        queryClient.setQueryData(settingsQueryKey, saved);
      }).catch(() => undefined);
    }, 180);
    return () => window.clearTimeout(openTimer);
  }, [location.pathname, onboardingOpen, overallOnboardingReady, pageGuide, queryClient, settingsQuery.data]);

  useEffect(() => {
    const startPageOnboarding = () => {
      const guide = getPageGuide(location.pathname);
      if (!guide) return;
      setOnboardingOpen(false);
      setPageGuide(guide);
    };
    window.addEventListener(START_PAGE_ONBOARDING_EVENT, startPageOnboarding);
    return () => window.removeEventListener(START_PAGE_ONBOARDING_EVENT, startPageOnboarding);
  }, [location.pathname]);

  useEffect(() => {
    let disposed = false;
    const poll = async () => {
      try {
        const session = await desktopApi.getGameSessionStatus();
        if (!disposed && analysisEnabled && session && !session.running && session.end && session.started_at !== dismissedSession) setCompletedSession(session);
      } catch { /* The rest of the shell remains usable when the desktop bridge is unavailable. */ }
    };
    const initial = window.setTimeout(() => void poll(), 0);
    const timer = window.setInterval(() => void poll(), 2000);
    return () => { disposed = true; window.clearTimeout(initial); window.clearInterval(timer); };
  }, [analysisEnabled, dismissedSession]);

  useEffect(() => {
    let disposed = false;
    let off: (() => void) | undefined;
    const handleStatus = (status: { clients: Array<{ client: Ruleset | "stable" | "lazer"; running: boolean }> }) => {
      if (!status.clients.some((client) => client.running) || disposed) return;
      status.clients.filter((client) => client.running).forEach((client) => {
        if (client.client === "stable" || client.client === "lazer") {
          void desktopApi.startDetectedGameSession(ruleset, client.client).catch(() => undefined);
        }
      });
    };
    void desktopApi.getGameStatus().then(handleStatus).catch(() => undefined);
    void desktopApi.onGameStatusChanged(handleStatus).then((unlisten) => { if (disposed) unlisten(); else off = unlisten; });
    return () => { disposed = true; off?.(); };
  }, [ruleset]);

  useEffect(() => {
    let disposed = false; let off: (() => void) | undefined;
    const handleObs = (status: { running: boolean }) => {
      if (!status.running || disposed) return;
      void desktopApi.getSettings().then((settings) => { if (!disposed && !settings.suppress_tosu_launch_prompt) setTosuPromptSettings(settings); }).catch(() => undefined);
    };
    void desktopApi.onObsStatusChanged(handleObs).then((unlisten) => { if (disposed) unlisten(); else off = unlisten; });
    return () => { disposed = true; off?.(); };
  }, []);

  useEffect(() => {
    const requestLaunch = () => { void desktopApi.getSettings().then((settings) => { if (settings.suppress_tosu_launch_prompt) void desktopApi.startTosu(); else setTosuPromptSettings(settings); }).catch(() => undefined); };
    window.addEventListener("opp:request-tosu-launch", requestLaunch);
    return () => window.removeEventListener("opp:request-tosu-launch", requestLaunch);
  }, []);

  useEffect(() => {
    const onScroll = () => setShowBackToTop(window.scrollY > 420);
    window.addEventListener("scroll", onScroll, { passive: true });
    onScroll();
    return () => window.removeEventListener("scroll", onScroll);
  }, []);

  useEffect(() => {
    const defaultMode = profileQuery.data?.data.playmode;
    if (
      !initializedMode.current &&
      defaultMode &&
      validRulesets.includes(defaultMode)
    ) {
      initializedMode.current = true;
      setRuleset(defaultMode);
    }
  }, [profileQuery.data, setRuleset]);

  useEffect(() => {
    const error = profileQuery.error as CommandError | null;
    if (error?.code === "AUTH_REQUIRED") {
      queryClient.invalidateQueries({ queryKey: authQueryKey });
    }
  }, [profileQuery.error, queryClient]);

  return (
    <div className="opp-app-shell min-h-screen">
      <a
        className="fixed left-[calc(var(--sidebar-width)+16px)] top-2 z-[200] -translate-y-20 rounded-lg bg-[var(--theme-primary)] px-4 py-2 text-sm font-semibold text-[var(--on-primary)] transition-transform focus:translate-y-0"
        href="#main-content"
      >
        跳到主要内容
      </a>
      <Sidebar
        loading={profileQuery.isLoading}
        profile={profileQuery.data?.data}
      />
      <GlobalContextBar />
      <main className="ml-[var(--sidebar-width)] min-h-screen pt-[108px]" id="main-content" tabIndex={-1}>
        <div className="relative min-h-[calc(100vh-108px)] overflow-x-clip">
          <div
            className={`theme-content-frame relative mx-auto max-w-[var(--content-width)] p-7 xl:p-9 ${refinedWorkspace ? "opp-refined-workspace" : ""}`}
            data-page-guide-content="true"
          >
            <Outlet />
          </div>
        </div>
      </main>
      {showBackToTop ? (
        <button
          aria-label="回到顶部"
          className="fixed bottom-24 right-7 z-[70] grid size-11 place-items-center rounded-lg border border-white/10 bg-[var(--surface-panel)] text-[var(--theme-primary)] shadow-xl transition-colors hover:border-[var(--theme-primary-soft)] hover:bg-[var(--theme-primary-muted)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--theme-primary)]"
          onClick={() => window.scrollTo({ top: 0, behavior: "smooth" })}
          type="button"
        >
          <ArrowUp className="size-5" />
        </button>
      ) : null}
      {completedSession || newReplays ? <><GameCompletionOverlay key={newReplays?.detected_at ?? completedSession?.started_at} session={completedSession} discovery={newReplays} settings={settingsQuery.data} onNavigate={(path) => navigate(path)} onClose={() => { if (completedSession) setDismissedSession(completedSession.started_at); setCompletedSession(null); setNewReplays(null); }} /><div className="fixed bottom-8 left-1/2 z-[110] -translate-x-1/2 rounded-xl border border-cyan-300/15 bg-[#0b101b]/95 px-4 py-2 text-xs text-slate-400 shadow-xl">Tips：嘛，如果拘泥于数据就会让游戏本来的乐趣消失哦</div></> : null}
      {tosuPromptSettings ? <TosuLaunchPrompt settings={tosuPromptSettings} onClose={() => setTosuPromptSettings(null)} /> : null}
      {onboardingOpen ? (
        <OnboardingTour
          onClose={() => {
            setOnboardingOpen(false);
            setOverallOnboardingReady(true);
          }}
          reduceMotion={settingsQuery.data?.reduce_motion ?? false}
        />
      ) : null}
      {pageGuide ? (
        <OnboardingTour
          eyebrow={`${pageGuide.title} · 页面引导`}
          onClose={() => setPageGuide(null)}
          reduceMotion={settingsQuery.data?.reduce_motion ?? false}
          steps={pageGuide.steps}
        />
      ) : null}
      <DownloadToast />
      <CollectionTaskToast />
      <DownloadCompletedPlaylist />
      <CollectionAddDialog defaultCreator={profileQuery.data?.data.username ?? ""} />
      <UpdateCenter
        autoCheckReady={Boolean(settingsQuery.data) && overallOnboardingReady && !onboardingOpen && !pageGuide}
        ignoredVersion={settingsQuery.data?.ignored_update_version}
      />
    </div>
  );
}
