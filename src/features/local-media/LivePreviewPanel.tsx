import type { LiveExportProgress } from "./renderEvents";
import * as Dialog from "@radix-ui/react-dialog";
import { save } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useRef, useState } from "react";
import { Film, FolderOpen, LoaderCircle, MonitorPlay, Pause, Play, Square, X } from "lucide-react";
import { useMode } from "../../app/ModeContext";
import { ErrorPanel } from "../../shared/components/ErrorPanel";
import { Button, Card, EmptyState, SectionTitle } from "../../shared/components/ui";
import { desktopApi, type LiveExportParams, type LiveRenderOptions, type LiveSkinEntry } from "../../shared/lib/tauri";
import { useReplayWorkspace, useRenderSession } from "./api";
import { replayBlockReason } from "./model";
import { ReplayIdentity, StudioTasks } from "./ReplayWorkspace";

function formatTime(ms: number) {
  const total = Math.max(0, Math.round(ms / 1000));
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
}

const defaultOptions: LiveRenderOptions = {
  hud: true,
  storyboard: false,
  video: false,
  urBar: true,
  followPoints: true,
  keyOverlay: true,
  ppDisplay: true,
  bg: true,
  bgOpacity: 0.3,
  audio: true,
  audioOffset: 0,
  hitsounds: true,
  cursorSize: 1,
  skinPath: null,
  skinColours: false,
  avatarPath: null,
};

const defaultExportForm = { resolution: "1920x1080", fps: 60, encoder: "x264" as LiveExportParams["encoder"], quality: 18, audio: true, hitsounds: true, results: true, audioOffset: 0 };

export function LivePreviewPanel({ visible }: { visible: boolean }) {
  const { client } = useMode();
  const { replayPath, replayInfo, inspectError } = useReplayWorkspace();
  const blocked = replayBlockReason(replayPath, replayInfo, "live", inspectError);
  // 音频偏移的原始输入:text 框允许键入 "-" 等中间态,解析成功才提交数值。
  const [audioOffsetText, setAudioOffsetText] = useRenderSession(`live-audio-offset:${client}`, String(defaultOptions.audioOffset));
  const [starting, setStarting] = useState(false);
  const [active, setActive] = useState(false);
  const [playing, setPlaying] = useState(false);
  const [duration, setDuration] = useState(0);
  const [time, setTime] = useState(0);
  const [scrubbing, setScrubbing] = useState(false);
  const [error, setError] = useRenderSession<unknown>("live-error", null);
  const [exportOpen, setExportOpen] = useState(false);
  const [ffmpegVersion, setFfmpegVersion] = useState<string | null | undefined>(undefined);
  // [h264_nvenc, hevc_nvenc] 可用性(undefined = 未探测)。
  const [nvenc, setNvenc] = useState<[boolean, boolean] | undefined>(undefined);
  const [exportForm, setExportForm] = useRenderSession("live-export-form", defaultExportForm);
  // 导出偏移的原始输入(与预览偏移同理:text 框允许键入 "-" 等中间态)。
  const [exportOffsetText, setExportOffsetText] = useRenderSession("live-export-offset", "0");
  const [exporting, setExporting] = useRenderSession<LiveExportProgress | null>("live-export-progress", null);
  const [exportResult, setExportResult] = useRenderSession<string | null>("live-export-result", null);
  const [exportBusy, setExportBusy] = useRenderSession("live-export-busy", false);
  const [options, setOptions] = useRenderSession<LiveRenderOptions>(`live-options:${client}`, defaultOptions);
  // 客户端 Skins 目录下的可选皮肤(内置 Argon-Pro 为默认项,不在列表)。
  const [skins, setSkins] = useState<LiveSkinEntry[]>([]);
  // 皮肤热切换失败信息(加载错误时后端事件推送;当前皮肤保持不变)。
  const [skinError, setSkinError] = useState<string | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const activeRef = useRef(false);
  const generation = useRef(0);
  const visibleRef = useRef(visible);
  useEffect(() => { visibleRef.current = visible; }, [visible]);
  const startedOptionsRef = useRef<string>("");

  // 时间轴事件:后端每帧/状态变化推送当前时间。
  useEffect(() => {
    let disposed = false;
    let unlisten: () => void = () => undefined;
    desktopApi.onLiveRenderTime((state) => {
      if (!state.active) {
        activeRef.current = false;
        setPlaying(false);
        setActive(false);
        return;
      }
      setPlaying(state.playing);
      setDuration(state.durationMs);
      if (!scrubbing) setTime(state.timeMs);
    }).then((dispose) => { if (disposed) dispose(); else unlisten = dispose; });
    return () => { disposed = true; unlisten(); };
  }, [scrubbing]);

  // 皮肤列表(客户端切换时重拉;失败静默为空,仅剩内置项)。
  useEffect(() => {
    let mounted = true;
    desktopApi.liveRenderListSkins(client)
      .then((list) => { if (mounted) setSkins(list); })
      .catch(() => { if (mounted) setSkins([]); });
    return () => { mounted = false; };
  }, [client]);

  // 皮肤热切换失败提示(仅展示,不打断预览)。
  useEffect(() => {
    let disposed = false;
    let unlisten: () => void = () => undefined;
    let unlistenErr: () => void = () => undefined;
    desktopApi.onLiveRenderSkinError((message) => setSkinError(message))
      .then((dispose) => { if (disposed) dispose(); else unlisten = dispose; })
      .catch(() => undefined);
    // 渲染线程异常(如图集超出 GPU 纹理限制):后端已清理会话,前端
    // 复位预览状态并提示重开。
    desktopApi.onLiveRenderError((message) => {
      activeRef.current = false;
      setActive(false);
      setPlaying(false);
      setSkinError(message);
    })
      .then((dispose) => { if (disposed) dispose(); else unlistenErr = dispose; })
      .catch(() => undefined);
    return () => {
      disposed = true;
      unlisten();
      unlistenErr();
    };
  }, []);

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
        .liveRenderMove({
          x: box.x * d,
          y: box.y * d,
          width: box.width * d,
          height: box.height * d,
          viewport_width: window.innerWidth * d,
          viewport_height: window.innerHeight * d,
          suppressed,
        })
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
    const workspace = element.closest(".replay-studio");
    if (workspace) observer.observe(workspace);
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
    generation.current++;
    activeRef.current = false;
    setActive(false);
    setPlaying(false);
    void desktopApi.liveRenderClose().catch(() => undefined);
  }, []);

  useEffect(() => {
    if (!visible) {
      generation.current++;
      activeRef.current = false;
      void desktopApi.liveRenderClose().then(() => { setActive(false); setPlaying(false); }).catch(() => undefined);
    }
  }, [visible]);

  // 卸载时关闭预览。
  useEffect(() => () => { generation.current++; if (activeRef.current) void desktopApi.liveRenderClose().catch(() => undefined); }, []);

  const start = async () => {
    if (blocked || !replayInfo?.beatmap_resource_id) return;
    const attempt = ++generation.current;
    setStarting(true);
    setError(null);
    try {
      const fresh = await desktopApi.inspectGameReplay(client, replayPath);
      const reason = replayBlockReason(replayPath, fresh, "live");
      if (reason || !fresh.beatmap_resource_id) throw new Error(reason ?? "谱面不可用");
      const beatmapPath = await desktopApi.getLocalBeatmapPath(client, fresh.beatmap_resource_id);
      // 结算屏头像:按账号头像 URL 落盘缓存后取本地路径;失败不阻塞预览。
      let avatarPath: string | null = null;
      try {
        const profile = await desktopApi.getOwnProfile("osu", false);
        const data = profile?.data;
        if (data?.id && data?.avatar_url) {
          avatarPath = await desktopApi
            .liveRenderResolveAvatar(data.id, data.avatar_url)
            .catch(() => null);
        }
      } catch {
        avatarPath = null;
      }
      const openOptions: LiveRenderOptions = { ...options, avatarPath };
      const box = containerRef.current?.getBoundingClientRect();
      const d = window.devicePixelRatio || 1;
      const viewport = {
        viewport_width: window.innerWidth * d,
        viewport_height: window.innerHeight * d,
      };
      const rect = box
        ? { x: box.x * d, y: box.y * d, width: box.width * d, height: box.height * d, ...viewport }
        : { x: 0, y: 0, width: 0, height: 0, ...viewport };
      if (attempt !== generation.current || !visibleRef.current) return;
      const info = await desktopApi.liveRenderOpen(beatmapPath, replayPath, openOptions, rect);
      if (attempt !== generation.current || !visibleRef.current) { await desktopApi.liveRenderClose(); return; }
      startedOptionsRef.current = JSON.stringify(openOptions);
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

  useEffect(() => {
    generation.current++;
    if (activeRef.current) {
      activeRef.current = false;
      void desktopApi.liveRenderClose().then(() => { setActive(false); setPlaying(false); }).catch(() => undefined);
    }
  }, [replayPath, client]);

  const openExport = async () => {
    setError(null);
    if (exportBusy || exporting) { setExportOpen(true); return; }
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
    if (blocked || !replayInfo?.beatmap_resource_id || exportBusy) return;
    setExportBusy(true);
    setError(null);
    try {
      const fresh = await desktopApi.inspectGameReplay(client, replayPath);
      const reason = replayBlockReason(replayPath, fresh, "live");
      if (reason || !fresh.beatmap_resource_id) throw new Error(reason ?? "谱面不可用");
      const beatmapPath = await desktopApi.getLocalBeatmapPath(client, fresh.beatmap_resource_id);
      const base = replayPath.split(/[\\/]/).pop()?.replace(/\.osr$/i, "") ?? "replay";
      const out = await save({ defaultPath: `${base || "replay"}.mp4`, filters: [{ name: "MP4 视频", extensions: ["mp4"] }] });
      if (!out) return;
      const [width, height] = exportForm.resolution.split("x").map(Number);
      setExporting({ phase: "render", frame: 0, total: 0, message: "准备中…" });
      await desktopApi.liveRenderExport(beatmapPath, replayPath, options, {
        outPath: out, width, height, fps: exportForm.fps, encoder: exportForm.encoder, quality: exportForm.quality, audio: exportForm.audio, hitsounds: exportForm.hitsounds, results: exportForm.results, audioOffset: exportForm.audioOffset,
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

  return <div className="studio-panel">
    {error ? <div className="mb-5"><ErrorPanel error={error} /></div> : null}
    <div className="studio-grid">
      <div className="studio-preview">
        <Card className="p-5">
          <div ref={containerRef} className="studio-viewport relative w-full overflow-hidden" role="region" aria-label="回放预览">
            {!active ? <ReplayIdentity provider="live" /> : null}
          </div>
          <div className="mt-4 flex flex-wrap items-center gap-3">
            {active ? <>
              <Button size="sm" variant="primary" onClick={toggle}>{playing ? <Pause className="size-4" /> : <Play className="size-4" />}{playing ? "暂停" : "播放"}</Button>
              <Button size="sm" onClick={stop}><Square className="size-4" />停止</Button>
              <span className="font-mono text-xs text-slate-400">{formatTime(time)} / {formatTime(duration)}</span>
            </> : null}
            <Button size="sm" disabled={Boolean(blocked)} onClick={() => void openExport()}><Film className="size-4" />导出视频</Button>
            <input
              className="w-full accent-cyan-400"
              aria-label="回放播放进度"
              type="range"
              min={0}
              max={Math.max(duration, 1)}
              step={10}
              value={Math.min(time, duration)}
              disabled={!active}
              onPointerDown={() => setScrubbing(true)}
              onPointerUp={() => setScrubbing(false)}
              onPointerCancel={() => setScrubbing(false)}
              onBlur={() => setScrubbing(false)}
              onChange={(event) => seek(Number(event.target.value))}
            />
          </div>
        </Card>
      </div>
      <div className="studio-settings" aria-label="实时预览设置"><div className="studio-settings-scroll">
        <Card className="p-5">
          <SectionTitle title="画面与声音" description="设置实时生效，导出可单独配置输出质量。" />
          {starting ? <div className="mt-4 flex items-center gap-2 rounded-xl border border-white/10 p-4 text-sm text-slate-300"><LoaderCircle className="size-4 animate-spin" />正在加载谱面与回放…</div> : null}
          <div className="mt-5 space-y-4 border-t border-white/[0.06] pt-5">
            <h3 className="text-xs font-semibold uppercase tracking-wider text-slate-500" title="即时生效,无需重载">音频</h3>
            <label className="flex cursor-pointer items-center gap-2 rounded-lg border border-white/[0.06] bg-black/15 px-3 py-2 text-xs text-slate-300" title="谱面自带音频([General] AudioFilename)">
              <input className="accent-cyan-400" type="checkbox" checked={options.audio} onChange={(event) => update("audio", event.target.checked)} />播放 BGM
            </label>
            <label className="flex cursor-pointer items-center gap-2 rounded-lg border border-white/[0.06] bg-black/15 px-3 py-2 text-xs text-slate-300" title="命中音/combobreak,ArgonPro">
              <input className="accent-cyan-400" type="checkbox" checked={options.hitsounds} onChange={(event) => update("hitsounds", event.target.checked)} />播放音效
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
            <h3>皮肤与光标</h3>
            <div className="block text-xs text-slate-400" title="即时热切换,缺件回退 Argon">皮肤
              <select
                className="mt-2 min-w-0 w-full rounded-xl border border-white/10 bg-black/20 p-3 text-sm text-white"
                value={options.skinPath ?? ""}
                onChange={(event) => {
                  setSkinError(null);
                  update("skinPath", event.target.value === "" ? null : event.target.value);
                }}
              >
                <option value="">内置 Argon-Pro</option>
                {skins.map((skin) => <option key={skin.path} value={skin.path}>{skin.name}</option>)}
              </select>
              {skinError ? <p className="mt-1 text-[10px] leading-relaxed text-amber-300">{skinError}</p> : null}
            </div>
            {options.skinPath ? <label className="flex cursor-pointer items-center gap-2 rounded-lg border border-white/[0.06] bg-black/15 px-3 py-2 text-xs text-slate-300" title="默认使用谱面 [Colours] 的 combo 色(谱面未配色时才用皮肤色);开启后强制使用皮肤的 combo 色(stable 行为)">
              <input className="accent-cyan-400" type="checkbox" checked={options.skinColours} onChange={(event) => update("skinColours", event.target.checked)} />皮肤 combo 色
            </label> : null}
            <label className="block text-xs text-slate-400">光标大小 {Math.round(options.cursorSize * 100)}%
              <input
                className="mt-3 w-full accent-cyan-400"
                type="range"
                min={10}
                max={200}
                step={5}
                value={Math.round(options.cursorSize * 100)}
                onChange={(event) => update("cursorSize", Number(event.target.value) / 100)}
              />
            </label>
            <h3>背景</h3>
            <label className="flex cursor-pointer items-center gap-2 rounded-lg border border-white/[0.06] bg-black/15 px-3 py-2 text-xs text-slate-300">
              <input className="accent-cyan-400" type="checkbox" checked={options.bg} onChange={(event) => update("bg", event.target.checked)} />谱面背景图
            </label>
            <label className="flex cursor-pointer items-center gap-2 rounded-lg border border-white/[0.06] bg-black/15 px-3 py-2 text-xs text-slate-300" title="谱面故事板(.osu Events + 共享 .osb);开启后背景图让位。切换会重建会话(约 1 秒)">
              <input className="accent-cyan-400" type="checkbox" checked={options.storyboard} onChange={(event) => update("storyboard", event.target.checked)} />故事板
            </label>
            <label className="flex cursor-pointer items-center gap-2 rounded-lg border border-white/[0.06] bg-black/15 px-3 py-2 text-xs text-slate-300" title="背景视频(故事板 Video 元素);ffmpeg 管道逐帧解码,工具路径取设置页手动路径 > PATH > danser 发行包自带。开启后背景图让位">
              <input className="accent-cyan-400" type="checkbox" checked={options.video} onChange={(event) => update("video", event.target.checked)} />背景视频
            </label>
            <label className="block text-xs text-slate-400" title="同时作用于背景图/故事板/背景视频(osu! 背景暗化的反向);拖动即时生效">背景亮度 {Math.round(options.bgOpacity * 100)}%
              <input className="mt-3 w-full accent-cyan-400" type="range" min={0} max={100} value={Math.round(options.bgOpacity * 100)} onChange={(event) => update("bgOpacity", Number(event.target.value) / 100)} />
            </label>
            <h3>叠加信息</h3>
            <label className="flex cursor-pointer items-center gap-2 rounded-lg border border-white/[0.06] bg-black/15 px-3 py-2 text-xs text-slate-300" title="玩法 HUD 总开关:关闭后隐藏分数/准确率/连击/血条/UR 条/按键展示/PP 计数,物件与光标照常;预览与视频导出共用">
              <input className="accent-cyan-400" type="checkbox" checked={options.hud} onChange={(event) => update("hud", event.target.checked)} />HUD
            </label>
            <label className="flex cursor-pointer items-center gap-2 rounded-lg border border-white/[0.06] bg-black/15 px-3 py-2 text-xs text-slate-300" title="UR 条与数值">
              <input className="accent-cyan-400" type="checkbox" checked={options.urBar} onChange={(event) => update("urBar", event.target.checked)} />UR 显示
            </label>
            <label className="flex cursor-pointer items-center gap-2 rounded-lg border border-white/[0.06] bg-black/15 px-3 py-2 text-xs text-slate-300" title="Z/X/C 键与计数">
              <input className="accent-cyan-400" type="checkbox" checked={options.keyOverlay} onChange={(event) => update("keyOverlay", event.target.checked)} />按键输入展示
            </label>
            <label className="flex cursor-pointer items-center gap-2 rounded-lg border border-white/[0.06] bg-black/15 px-3 py-2 text-xs text-slate-300" title="游玩过程中的实时性能点数(逐物件渐增,Argon 样式挂在 ACC 行下方)">
              <input className="accent-cyan-400" type="checkbox" checked={options.ppDisplay} onChange={(event) => update("ppDisplay", event.target.checked)} />PP 计数
            </label>
            <label className="flex cursor-pointer items-center gap-2 rounded-lg border border-white/[0.06] bg-black/15 px-3 py-2 text-xs text-slate-300" title="Follow points">
              <input className="accent-cyan-400" type="checkbox" checked={options.followPoints} onChange={(event) => update("followPoints", event.target.checked)} />物件引导线
            </label>
          </div>
        </Card></div><div className="studio-action-bar"><p className="studio-disabled-reason">{blocked}</p><Button className="w-full studio-primary-action" variant="primary" loading={starting} disabled={Boolean(blocked) || active} onClick={() => void start()}>
            <MonitorPlay className="size-4" />开始预览
          </Button></div>
      </div>
    </div>
    <StudioTasks provider="live" title={exporting ? exporting.message : exportResult ? "导出完成" : active ? "实时预览中" : "等待预览或导出"}>
      {exporting ? <p>{exporting.message}<Button onClick={() => void desktopApi.liveRenderExportCancel()}>取消导出</Button></p> : exportResult ? <p>{exportResult}<Button onClick={() => void desktopApi.liveRenderOpenExportOutput(exportResult)}>打开所在文件夹</Button></p> : <EmptyState title="准备好记录精彩了吗？" description="选择回放后可开始预览，或直接导出视频。" />}
    </StudioTasks>
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
                <option value="x264">H.264</option>
                <option value="x265">H.265</option>
                <option value="nvenc" disabled={nvenc !== undefined && !nvenc[0]}>NVENC{nvenc !== undefined && !nvenc[0] ? "(不可用)" : ""}</option>
                <option value="hevc_nvenc" disabled={nvenc !== undefined && !nvenc[1]}>H.265 NVENC{nvenc !== undefined && !nvenc[1] ? "(不可用)" : ""}</option>
              </select>
            </label>
            <label className="block text-xs text-slate-400" title="crf 越低画质越高">质量 crf {exportForm.quality}
              <input className="mt-3 w-full accent-cyan-400" type="range" min={14} max={28} value={exportForm.quality} onChange={(event) => setExportForm((f) => ({ ...f, quality: Number(event.target.value) }))} />
            </label>
            <div className="space-y-1 rounded-lg border border-white/[0.06] bg-black/15 px-3 py-2">
              <label className="flex cursor-pointer items-center gap-2 text-xs text-slate-300" title="谱面自带音频,AAC 192k">
                <input className="accent-cyan-400" type="checkbox" checked={exportForm.audio} onChange={(event) => setExportForm((f) => ({ ...f, audio: event.target.checked }))} />混入 BGM
              </label>
              {exportForm.audio ? <label className="block pl-6 text-xs text-slate-400" title="与预览偏移互相独立">导出音频偏移 {exportOffsetText === "" ? 0 : exportOffsetText} ms
                <input
                  className="mt-2 w-full rounded-xl border border-white/10 bg-black/20 p-2 text-sm text-white"
                  type="text"
                  inputMode="numeric"
                  value={exportOffsetText}
                  onChange={(event) => {
                    const raw = event.target.value.replace(/[^\d.-]/g, "");
                    setExportOffsetText(raw);
                    if (raw === "") {
                      setExportForm((f) => ({ ...f, audioOffset: 0 }));
                      return;
                    }
                    const parsed = Number(raw);
                    if (raw !== "-" && Number.isFinite(parsed)) {
                      setExportForm((f) => ({ ...f, audioOffset: parsed }));
                    }
                  }}
                />
              </label> : null}
              <label className="flex cursor-pointer items-center gap-2 text-xs text-slate-300" title="命中音/combobreak,ArgonPro">
                <input className="accent-cyan-400" type="checkbox" checked={exportForm.hitsounds} onChange={(event) => setExportForm((f) => ({ ...f, hitsounds: event.target.checked }))} />混入音效
              </label>
                <label className="flex items-center gap-2 text-sm" title="玩法后追加 4 秒,默认开">
                  <input className="accent-cyan-400" type="checkbox" checked={exportForm.results} onChange={(event) => setExportForm((f) => ({ ...f, results: event.target.checked }))} />生成结算屏
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
