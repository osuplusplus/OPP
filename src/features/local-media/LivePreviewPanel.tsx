import * as Dialog from "@radix-ui/react-dialog";
import { save } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useRef, useState } from "react";
import { Film, FolderOpen, LoaderCircle, MonitorPlay, Pause, Play, Square, X } from "lucide-react";
import { useMode } from "../../app/ModeContext";
import { ErrorPanel } from "../../shared/components/ErrorPanel";
import { Badge, Button, Card, EmptyState, SectionTitle } from "../../shared/components/ui";
import { desktopApi, type LiveExportParams, type LiveRenderOptions } from "../../shared/lib/tauri";
import type { GameMediaItem, ReplayMapInfo } from "../../shared/types/osu";

function labelForReplay(item: GameMediaItem) {
  return item.path.split(/[\\/]/).pop() ?? item.path;
}

function formatTime(ms: number) {
  const total = Math.max(0, Math.round(ms / 1000));
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
}

const defaultOptions: LiveRenderOptions = {
  urBar: true,
  followPoints: true,
  keyOverlay: true,
  bg: false,
  bgOpacity: 0.3,
  audio: true,
  audioOffset: 0,
  hitsounds: true,
};

export function LivePreviewPanel() {
  const { client } = useMode();
  const [replays, setReplays] = useState<GameMediaItem[]>([]);
  const [replayPath, setReplayPath] = useState("");
  // 音频偏移的原始输入:text 框允许键入 "-" 等中间态,解析成功才提交数值。
  const [audioOffsetText, setAudioOffsetText] = useState(String(defaultOptions.audioOffset));
  const [inspect, setInspect] = useState<{ path: string; info: ReplayMapInfo | null }>({ path: "", info: null });
  const [loading, setLoading] = useState(true);
  const [starting, setStarting] = useState(false);
  const [active, setActive] = useState(false);
  const [playing, setPlaying] = useState(false);
  const [duration, setDuration] = useState(0);
  const [time, setTime] = useState(0);
  const [scrubbing, setScrubbing] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [exportOpen, setExportOpen] = useState(false);
  const [ffmpegVersion, setFfmpegVersion] = useState<string | null | undefined>(undefined);
  // [h264_nvenc, hevc_nvenc] 可用性(undefined = 未探测)。
  const [nvenc, setNvenc] = useState<[boolean, boolean] | undefined>(undefined);
  const [exportForm, setExportForm] = useState({ resolution: "1280x720", fps: 60, encoder: "x264" as LiveExportParams["encoder"], quality: 18, audio: true, hitsounds: true });
  const [exporting, setExporting] = useState<{ phase: string; frame: number; total: number; message: string } | null>(null);
  const [exportResult, setExportResult] = useState<string | null>(null);
  const [exportBusy, setExportBusy] = useState(false);
  const [options, setOptions] = useState<LiveRenderOptions>(defaultOptions);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const activeRef = useRef(false);
  const startedOptionsRef = useRef<string>("");
  const startedInputsRef = useRef<{ beatmap: string; replay: string }>({ beatmap: "", replay: "" });

  // 时间轴事件:后端每帧/状态变化推送当前时间。
  useEffect(() => {
    let unlisten: () => void = () => undefined;
    desktopApi.onLiveRenderTime((state) => {
      if (!state.active) {
        setActive(false);
        return;
      }
      setPlaying(state.playing);
      setDuration(state.durationMs);
      if (!scrubbing) setTime(state.timeMs);
    }).then((dispose) => { unlisten = dispose; });
    return () => unlisten();
  }, [scrubbing]);

  // 导出进度事件。done 是终态:清空 exporting 才能让弹窗从进度界面
  // 切到"导出完成"(渲染分支里 exporting 优先于 exportResult)。
  useEffect(() => {
    let unlisten: () => void = () => undefined;
    desktopApi.onLiveRenderExport((progress) => {
      if (progress.phase === "done") {
        setExporting(null);
        setExportResult(progress.message);
        return;
      }
      setExporting(progress);
    }).then((dispose) => { unlisten = dispose; });
    return () => unlisten();
  }, []);

  // 回放列表 + inspect(复用 o!rdr 面板的数据链路)。
  useEffect(() => {
    let mounted = true;
    desktopApi.listGameMedia(client)
      .then((media) => {
        if (!mounted) return;
        const items = media.filter((item) => item.kind === "replay");
        setReplays(items);
        setReplayPath(items[0]?.path ?? "");
      })
      .catch((value) => { if (mounted) setError(value); })
      .finally(() => { if (mounted) setLoading(false); });
    return () => { mounted = false; };
  }, [client]);

  const replayInfo = inspect.path === replayPath ? inspect.info : null;

  useEffect(() => {
    if (!replayPath) return;
    let mounted = true;
    desktopApi.inspectGameReplay(client, replayPath)
      .then((info) => { if (mounted) setInspect({ path: replayPath, info }); })
      .catch(() => { if (mounted) setInspect({ path: replayPath, info: null }); });
    return () => { mounted = false; };
  }, [client, replayPath]);

  // 原生模式:上报预览区域位置,原生子窗口跟随 DOM 元素(滚动/缩放)。
  // 原生窗口压在 WebView 之上,会盖住应用内弹窗(对话框/确认框):
  // 检测到弹窗打开时附带 suppressed,后端临时隐藏预览窗口。
  useEffect(() => {
    if (!active) return;
    const element = containerRef.current;
    if (!element) return;
    let raf = 0;
    let pending = false;
    let suppressed = false;
    const dialogOpen = () =>
      document.querySelector('[role="dialog"]') !== null ||
      document.body.hasAttribute("data-scroll-locked");
    const push = () => {
      pending = false;
      const box = element.getBoundingClientRect();
      // 物理像素:WebKitGTK 在 X11 小数缩放(Xft.dpi)下 devicePixelRatio
      // 是小数(如 1.25),而后端能拿到的 tauri scale_factor 只是 GDK
      // 整数缩放(=1)——坐标换算只能以 dpr 为准(Windows 的 WebView2
      // 同样满足 dpr == scale_factor,行为不变)。
      const d = window.devicePixelRatio || 1;
      void desktopApi
        .liveRenderMove({ x: box.x * d, y: box.y * d, width: box.width * d, height: box.height * d, suppressed })
        .catch(() => undefined);
    };
    const schedule = () => {
      if (pending) return;
      pending = true;
      raf = requestAnimationFrame(push);
    };
    const detectDialog = () => {
      const next = dialogOpen();
      if (next !== suppressed) {
        suppressed = next;
        schedule();
      }
    };
    const observer = new ResizeObserver(schedule);
    observer.observe(element);
    // 弹窗增删/属性变化都会触发(radix 开关 dialog 改 aria/data 属性)。
    const mutation = new MutationObserver(detectDialog);
    mutation.observe(document.body, { childList: true, subtree: true, attributes: true });
    window.addEventListener("resize", schedule);
    window.addEventListener("scroll", schedule, true);
    push();
    return () => {
      cancelAnimationFrame(raf);
      observer.disconnect();
      mutation.disconnect();
      window.removeEventListener("resize", schedule);
      window.removeEventListener("scroll", schedule, true);
    };
  }, [active]);

  const stop = useCallback(() => {
    activeRef.current = false;
    setActive(false);
    setPlaying(false);
    void desktopApi.liveRenderClose().catch(() => undefined);
  }, []);

  // 卸载时关闭预览。
  useEffect(() => () => { if (activeRef.current) void desktopApi.liveRenderClose().catch(() => undefined); }, []);

  const start = async () => {
    if (!replayPath || !replayInfo?.beatmap_resource_id) return;
    setStarting(true);
    setError(null);
    try {
      const beatmapPath = await desktopApi.getLocalBeatmapPath(client, replayInfo.beatmap_resource_id);
      const box = containerRef.current?.getBoundingClientRect();
      const d = window.devicePixelRatio || 1;
      const rect = box ? { x: box.x * d, y: box.y * d, width: box.width * d, height: box.height * d } : { x: 0, y: 0, width: 0, height: 0 };
      const info = await desktopApi.liveRenderOpen(beatmapPath, replayPath, options, rect);
      startedOptionsRef.current = JSON.stringify(options);
      startedInputsRef.current = { beatmap: beatmapPath, replay: replayPath };
      activeRef.current = true;
      setActive(true);
      setDuration(info.durationMs);
      setTime(0);
    } catch (value) {
      setError(value);
    } finally {
      setStarting(false);
    }
  };

  // 渲染参数变化:原地生效(零重载),滑条/数字框连续输入做防抖。
  const optionsKey = JSON.stringify(options);
  useEffect(() => {
    if (!activeRef.current) return;
    if (optionsKey === startedOptionsRef.current) return;
    startedOptionsRef.current = optionsKey;
    const timer = window.setTimeout(() => {
      void desktopApi.liveRenderSetOptions(options).catch(() => undefined);
    }, 250);
    return () => window.clearTimeout(timer);
    // options 为最新状态;防抖期间变化会重设定时器。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [optionsKey]);

  // 素材变化(换回放文件):判定数据需重算,重启会话。
  useEffect(() => {
    if (!activeRef.current) return;
    if (startedInputsRef.current.replay === replayPath) return;
    void desktopApi.liveRenderClose().catch(() => undefined);
    activeRef.current = false;
    void start();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [replayPath]);

  const openExport = async () => {
    setError(null);
    setExportResult(null);
    setExporting(null);
    try {
      const version = await desktopApi.liveRenderCheckFfmpeg();
      setFfmpegVersion(version);
      const [h264, hevc] = await desktopApi.liveRenderCheckNvenc();
      setNvenc([h264, hevc]);
      setExportForm((f) =>
        (f.encoder === "nvenc" && !h264) || (f.encoder === "hevc_nvenc" && !hevc)
          ? { ...f, encoder: "x264" }
          : f,
      );
      if (playing) {
        setPlaying(false);
        void desktopApi.liveRenderPause();
      }
      setExportOpen(true);
    } catch (value) {
      setError(value);
    }
  };

  const confirmExport = async () => {
    if (!replayPath || !replayInfo?.beatmap_resource_id) return;
    setExportBusy(true);
    setError(null);
    try {
      const beatmapPath = await desktopApi.getLocalBeatmapPath(client, replayInfo.beatmap_resource_id);
      const base = replayPath.split(/[\\/]/).pop()?.replace(/\.osr$/i, "") ?? "replay";
      const out = await save({ defaultPath: `${base || "replay"}.mp4`, filters: [{ name: "MP4 视频", extensions: ["mp4"] }] });
      if (!out) return;
      const [width, height] = exportForm.resolution.split("x").map(Number);
      setExporting({ phase: "render", frame: 0, total: 0, message: "准备中…" });
      await desktopApi.liveRenderExport(beatmapPath, replayPath, options, {
        outPath: out, width, height, fps: exportForm.fps, encoder: exportForm.encoder, quality: exportForm.quality, audio: exportForm.audio, hitsounds: exportForm.hitsounds,
      });
    } catch (value) {
      setError(value);
      setExporting(null);
    } finally {
      setExportBusy(false);
    }
  };

  const update = <K extends keyof LiveRenderOptions>(key: K, value: LiveRenderOptions[K]) =>
    setOptions((current) => ({ ...current, [key]: value }));

  const toggle = () => {
    setPlaying(!playing);
    void (playing ? desktopApi.liveRenderPause() : desktopApi.liveRenderPlay());
  };
  const seek = (value: number) => {
    setTime(value);
    void desktopApi.liveRenderSeek(value);
  };

  return <div className="pb-8">
    {error ? <div className="mb-5"><ErrorPanel error={error} onRetry={() => void start()} /></div> : null}
    <div className="grid gap-5 xl:grid-cols-[minmax(0,1fr)_360px]">
      <div className="space-y-5">
        <Card className="p-5">
          <SectionTitle
            title="预览区域"
            description="原生窗口直渲(wgpu 直接呈现,高帧率),覆盖在下方区域。"
          />
          <div ref={containerRef} className="relative mt-5 aspect-video w-full overflow-hidden rounded-xl border border-white/10 bg-[#0e0e13]">
            {!active ? <div className="absolute inset-0 flex items-center justify-center text-sm text-slate-600">开始预览后画面显示在这里</div> : null}
          </div>
          <div className="mt-4 flex flex-wrap items-center gap-3">
            {active ? <>
              <Button size="sm" variant="primary" onClick={toggle}>{playing ? <Pause className="size-4" /> : <Play className="size-4" />}{playing ? "暂停" : "播放"}</Button>
              <Button size="sm" onClick={stop}><Square className="size-4" />停止</Button>
              <span className="font-mono text-xs text-slate-400">{formatTime(time)} / {formatTime(duration)}</span>
            </> : null}
            <Button size="sm" disabled={!replayPath || !replayInfo?.beatmap_resource_id} onClick={() => void openExport()}><Film className="size-4" />导出视频</Button>
            <input
              className="w-full accent-cyan-400"
              type="range"
              min={0}
              max={Math.max(duration, 1)}
              step={10}
              value={Math.min(time, duration)}
              disabled={!active}
              onPointerDown={() => setScrubbing(true)}
              onPointerUp={() => setScrubbing(false)}
              onChange={(event) => seek(Number(event.target.value))}
            />
          </div>
        </Card>
      </div>
      <div className="space-y-5">
        <Card className="p-5">
          <SectionTitle title="素材" description="从当前客户端的 Replays 目录选择;谱面需已建立本地索引。" />
          <select
            className="mt-5 w-full rounded-xl border border-white/10 bg-black/20 p-3 text-base text-white"
            disabled={loading || active}
            value={replayPath}
            onChange={(event) => setReplayPath(event.target.value)}
          >
            <option value="">{loading ? "正在扫描本地回放…" : "选择回放"}</option>
            {replays.map((item) => <option key={item.path} value={item.path}>{labelForReplay(item)}</option>)}
          </select>
          {replayInfo ? <div className={`mt-4 rounded-xl border p-4 text-sm ${replayInfo.beatmap_resource_id ? "border-success-300/15 bg-success-300/[0.05] text-success-100" : "border-amber-300/15 bg-amber-300/[0.05] text-amber-100"}`}>
            {replayInfo.beatmap_resource_id ? `已匹配谱面 · ${replayInfo.beatmap_title ?? `Beatmap ${replayInfo.beatmap_id ?? ""}`}` : "未在本地索引中找到对应谱面,无法预览。"}
          </div> : null}
          {starting ? <div className="mt-4 flex items-center gap-2 rounded-xl border border-white/10 p-4 text-sm text-slate-300"><LoaderCircle className="size-4 animate-spin" />正在加载谱面与回放…</div> : null}
          <div className="mt-5 space-y-4 border-t border-white/[0.06] pt-5">
            <h3 className="text-xs font-semibold uppercase tracking-wider text-slate-500">渲染选项(即时生效,无需重载)</h3>
            <label className="flex cursor-pointer items-center gap-2 rounded-lg border border-white/[0.06] bg-black/15 px-3 py-2 text-xs text-slate-300">
              <input className="accent-cyan-400" type="checkbox" checked={options.audio} onChange={(event) => update("audio", event.target.checked)} />播放 BGM(谱面自带音频)
            </label>
            <label className="flex cursor-pointer items-center gap-2 rounded-lg border border-white/[0.06] bg-black/15 px-3 py-2 text-xs text-slate-300">
              <input className="accent-cyan-400" type="checkbox" checked={options.hitsounds} onChange={(event) => update("hitsounds", event.target.checked)} />播放音效(命中音/combobreak,ArgonPro)
            </label>
            {options.audio ? <label className="block text-xs text-slate-400">音频偏移 {audioOffsetText === "" ? 0 : audioOffsetText} ms
              <input
                className="mt-2 w-full rounded-xl border border-white/10 bg-black/20 p-3 text-sm text-white"
                type="text"
                inputMode="numeric"
                value={audioOffsetText}
                onChange={(event) => {
                  const raw = event.target.value.replace(/[^\d.-]/g, "");
                  setAudioOffsetText(raw);
                  if (raw === "") {
                    update("audioOffset", 0);
                    return;
                  }
                  const parsed = Number(raw);
                  if (raw !== "-" && Number.isFinite(parsed)) {
                    update("audioOffset", parsed);
                  }
                }}
              />
            </label> : null}
            <label className="flex cursor-pointer items-center gap-2 rounded-lg border border-white/[0.06] bg-black/15 px-3 py-2 text-xs text-slate-300">
              <input className="accent-cyan-400" type="checkbox" checked={options.bg} onChange={(event) => update("bg", event.target.checked)} />谱面背景图
            </label>
            {options.bg ? <label className="block text-xs text-slate-400">背景不透明度 {Math.round(options.bgOpacity * 100)}%
              <input className="mt-3 w-full accent-cyan-400" type="range" min={0} max={100} value={Math.round(options.bgOpacity * 100)} onChange={(event) => update("bgOpacity", Number(event.target.value) / 100)} />
            </label> : null}
            <label className="flex cursor-pointer items-center gap-2 rounded-lg border border-white/[0.06] bg-black/15 px-3 py-2 text-xs text-slate-300">
              <input className="accent-cyan-400" type="checkbox" checked={options.urBar} onChange={(event) => update("urBar", event.target.checked)} />UR 显示(UR 条与数值)
            </label>
            <label className="flex cursor-pointer items-center gap-2 rounded-lg border border-white/[0.06] bg-black/15 px-3 py-2 text-xs text-slate-300">
              <input className="accent-cyan-400" type="checkbox" checked={options.keyOverlay} onChange={(event) => update("keyOverlay", event.target.checked)} />按键输入展示(Z/X/C 键与计数)
            </label>
            <label className="flex cursor-pointer items-center gap-2 rounded-lg border border-white/[0.06] bg-black/15 px-3 py-2 text-xs text-slate-300">
              <input className="accent-cyan-400" type="checkbox" checked={options.followPoints} onChange={(event) => update("followPoints", event.target.checked)} />物件引导线(Follow points)
            </label>
          </div>
          <Button className="mt-5 w-full" variant="primary" loading={starting} disabled={!replayPath || !replayInfo?.beatmap_resource_id || active} onClick={() => void start()}>
            <MonitorPlay className="size-4" />开始预览
          </Button>
        </Card>
        {!active ? <Card className="p-5"><EmptyState icon={<MonitorPlay className="size-5" />} title="等待预览" description="选择回放并点击“开始预览”后,可播放、暂停并任意拖动进度条。" /></Card> : <Card className="p-5"><div className="flex items-center justify-between"><SectionTitle title="预览中" description="渲染在本机 GPU 上实时进行。" /><Badge tone="cyan">原生直渲</Badge></div></Card>}
      </div>
    </div>
    <Dialog.Root open={exportOpen} onOpenChange={(open) => { if (!exportBusy) setExportOpen(open); }}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-[80] bg-black/55 backdrop-blur-md" />
        <Dialog.Content className="fixed left-1/2 top-1/2 z-[90] w-[min(560px,calc(100vw-2rem))] -translate-x-1/2 -translate-y-1/2 rounded-2xl border border-white/10 bg-[#12121a] p-6 outline-none">
          <div className="flex items-center justify-between">
            <Dialog.Title className="text-lg font-semibold text-white">导出回放视频</Dialog.Title>
            <Dialog.Close asChild disabled={exportBusy}><button className="rounded-lg p-1 text-slate-500 hover:text-slate-200" type="button"><X className="size-4" /></button></Dialog.Close>
          </div>
          {ffmpegVersion === null ? <div className="mt-5 rounded-xl border border-amber-300/15 bg-amber-300/[0.05] p-4 text-sm text-amber-100">未检测到 FFmpeg。请安装 FFmpeg 并确保 ffmpeg 在 PATH 中,然后重试。</div> : exporting ? (
            <div className="mt-5 space-y-4">
              <div className="flex items-center gap-2 text-sm text-slate-300"><LoaderCircle className="size-4 animate-spin" />{exporting.phase === "mux" ? "混入音频…" : `正在渲染 ${exporting.frame}/${exporting.total} 帧`}</div>
              <div className="h-2 w-full overflow-hidden rounded-full bg-black/30"><div className="h-full rounded-full bg-cyan-400 transition-all" style={{ width: exporting.total > 0 ? `${Math.round((exporting.frame / exporting.total) * 100)}%` : "0%" }} /></div>
              <Button size="sm" onClick={() => void desktopApi.liveRenderExportCancel()}>取消导出</Button>
            </div>
          ) : exportResult ? (
            <div className="mt-5 space-y-4">
              <div className="rounded-xl border border-emerald-300/15 bg-emerald-300/[0.05] p-4 text-sm text-emerald-100">导出完成:{exportResult}</div>
              <div className="flex gap-2">
                <Button size="sm" onClick={() => void desktopApi.liveRenderOpenExportOutput(exportResult)}><FolderOpen className="size-4" />打开所在文件夹</Button>
                <Button size="sm" onClick={() => setExportOpen(false)}>关闭</Button>
              </div>
            </div>
          ) : <div className="mt-5 space-y-4">
            {ffmpegVersion ? <div className="rounded-xl border border-white/[0.06] bg-black/15 px-3 py-2 font-mono text-xs text-slate-500">{ffmpegVersion}</div> : null}
            <div className="grid grid-cols-2 gap-3">
              <label className="text-xs text-slate-400">分辨率
                <select className="mt-2 w-full rounded-xl border border-white/10 bg-black/20 p-3 text-sm text-white" value={exportForm.resolution} onChange={(event) => setExportForm((f) => ({ ...f, resolution: event.target.value }))}>
                  <option value="1280x720">1280×720</option>
                  <option value="1920x1080">1920×1080</option>
                </select>
              </label>
              <label className="text-xs text-slate-400">帧率
                <select className="mt-2 w-full rounded-xl border border-white/10 bg-black/20 p-3 text-sm text-white" value={exportForm.fps} onChange={(event) => setExportForm((f) => ({ ...f, fps: Number(event.target.value) }))}>
                  <option value={60}>60 fps</option>
                  <option value={30}>30 fps</option>
                </select>
              </label>
            </div>
            <label className="block text-xs text-slate-400">编码器
              <select className="mt-2 w-full rounded-xl border border-white/10 bg-black/20 p-3 text-sm text-white" value={exportForm.encoder} onChange={(event) => setExportForm((f) => ({ ...f, encoder: event.target.value as LiveExportParams["encoder"] }))}>
                <option value="x264">H.264(x264,兼容性最好)</option>
                <option value="x265">H.265(x265,体积更小)</option>
                <option value="nvenc" disabled={nvenc !== undefined && !nvenc[0]}>NVENC(NVIDIA 硬件编码,最快){nvenc !== undefined && !nvenc[0] ? "(不可用)" : ""}</option>
                <option value="hevc_nvenc" disabled={nvenc !== undefined && !nvenc[1]}>H.265 NVENC(NVIDIA 硬件编码,快且体积小){nvenc !== undefined && !nvenc[1] ? "(不可用)" : ""}</option>
              </select>
            </label>
            <label className="block text-xs text-slate-400">质量(crf {exportForm.quality},越低画质越高)
              <input className="mt-3 w-full accent-cyan-400" type="range" min={14} max={28} value={exportForm.quality} onChange={(event) => setExportForm((f) => ({ ...f, quality: Number(event.target.value) }))} />
            </label>
            <div className="space-y-1 rounded-lg border border-white/[0.06] bg-black/15 px-3 py-2">
              <label className="flex cursor-pointer items-center gap-2 text-xs text-slate-300">
                <input className="accent-cyan-400" type="checkbox" checked={exportForm.audio} onChange={(event) => setExportForm((f) => ({ ...f, audio: event.target.checked }))} />混入 BGM(谱面自带音频,AAC 192k)
              </label>
              <label className="flex cursor-pointer items-center gap-2 text-xs text-slate-300">
                <input className="accent-cyan-400" type="checkbox" checked={exportForm.hitsounds} onChange={(event) => setExportForm((f) => ({ ...f, hitsounds: event.target.checked }))} />混入音效(命中音/combobreak,ArgonPro)
              </label>
              <p className="pl-6 text-[10px] leading-relaxed text-slate-500">音量按 osu! 默认值(Music/Effect/Master 各 60%),两者同时混入时自动混合为一条音轨</p>
            </div>
            <Button className="w-full" variant="primary" loading={exportBusy} disabled={ffmpegVersion === null} onClick={() => void confirmExport()}><Film className="size-4" />选择保存位置并导出</Button>
          </div>}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  </div>;
}
