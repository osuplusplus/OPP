import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Download,
  Gauge,
  Lock,
  Music4,
  Pause,
  Play,
  Square,
  Unlock,
  WandSparkles,
} from "lucide-react";
import { useSearchParams } from "react-router-dom";
import { PageHeader } from "../../shared/components/PageHeader";
import {
  Button,
  Card,
  EmptyState,
  SectionTitle,
} from "../../shared/components/ui";
import { desktopApi } from "../../shared/lib/tauri";
import type {
  OsuClient,
  ViewTrainerRequest,
  ViewTrainerTimeline,
} from "../../shared/types/osu";

type ViewTrainerDraft = {
  rate: number;
  bpm_locked: boolean;
  target_bpm: number | null;
  scale_ar: boolean;
  scale_od: boolean;
  lock_ar: boolean;
  lock_od: boolean;
  lock_cs: boolean;
  lock_hp: boolean;
  ar: number;
  od: number;
  cs: number;
  hp: number;
  min_bpm: number | null;
  max_bpm: number | null;
  start_time_ms: number | null;
  end_time_ms: number | null;
  no_spinners: boolean;
  change_pitch: boolean;
  window_ms: number;
};
const DEFAULT_DRAFT: ViewTrainerDraft = {
  rate: 1,
  bpm_locked: false,
  target_bpm: null,
  scale_ar: true,
  scale_od: true,
  lock_ar: false,
  lock_od: false,
  lock_cs: false,
  lock_hp: false,
  ar: 5,
  od: 5,
  cs: 4,
  hp: 5,
  min_bpm: null,
  max_bpm: null,
  start_time_ms: null,
  end_time_ms: null,
  no_spinners: false,
  change_pitch: false,
  window_ms: 30_000,
};
const clamp = (value: number, min: number, max: number) =>
  Math.min(max, Math.max(min, value));
const formatTime = (ms: number) => {
  const total = Math.max(0, Math.round(ms / 1000));
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, "0")}`;
};
const arToMs = (ar: number) =>
  ar <= 5 ? 1800 - ar * 120 : 1200 - (ar - 5) * 150;
const scaledAr = (ar: number, rate: number) =>
  clamp(
    arToMs(ar) / rate >= 1200
      ? (1800 - arToMs(ar) / rate) / 120
      : 5 + (1200 - arToMs(ar) / rate) / 150,
    0,
    11,
  );
const scaledOd = (od: number, rate: number) =>
  Math.round(clamp((79.5 - (-6 * od + 79.5) / rate) / 6, 0, 11) * 10) / 10;
function numberValue(value: string, fallback: number | null) {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : fallback;
}
function ToggleButton({
  locked,
  onClick,
  label,
}: {
  locked: boolean;
  onClick: () => void;
  label: string;
}) {
  const Icon = locked ? Lock : Unlock;
  return (
    <button
      aria-label={`${label}${locked ? "已锁定" : "未锁定"}`}
      className="grid size-8 place-items-center rounded border border-white/10 text-slate-400 hover:text-white"
      onClick={onClick}
      title={`${label}${locked ? "已锁定" : "未锁定"}`}
      type="button"
    >
      <Icon className="size-3.5" />
    </button>
  );
}

export function ViewTrainerPage() {
  const [params] = useSearchParams();
  const client = (
    params.get("client") === "lazer" ? "lazer" : "stable"
  ) as OsuClient;
  const resourceId = params.get("resource") ?? "";
  const [timeline, setTimeline] = useState<ViewTrainerTimeline | null>(null);
  const [draft, setDraft] = useState<ViewTrainerDraft>(DEFAULT_DRAFT);
  const [generating, setGenerating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [generatedPath, setGeneratedPath] = useState<string | null>(null);
  const [stagedDirectory, setStagedDirectory] = useState<string | null>(null);
  const [importedPath, setImportedPath] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);
  const [active, setActive] = useState(false);
  const [playing, setPlaying] = useState(false);
  const [time, setTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const previewRef = useRef<HTMLDivElement | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const activeRef = useRef(false);
  const requestVersion = useRef(0);
  const previewQueue = useRef<Promise<void>>(Promise.resolve());
  const generatingRef = useRef(false);
  const dragHandle = useRef<"start" | "end" | "seek" | null>(null);
  const updateDraft = useCallback(
    (patch: Partial<ViewTrainerDraft>) =>
      setDraft((current) => ({ ...current, ...patch })),
    [],
  );
  useEffect(() => {
    if (!resourceId) return;
    let cancelled = false;
    requestVersion.current += 1;
    activeRef.current = false;
    queueMicrotask(() => {
      if (cancelled) return;
      setActive(false);
      setPlaying(false);
      setTimeline(null);
      setDraft(DEFAULT_DRAFT);
      setError(null);
      setGeneratedPath(null);
      setStagedDirectory(null);
      setImportedPath(null);
      setImporting(false);
    });
    void desktopApi.liveRenderClose().catch(() => undefined);
    void desktopApi
      .viewTrainerGetTimeline(client, resourceId)
      .then((value) => {
        if (cancelled) return;
        setTimeline(value);
        setDuration(value.durationMs);
        setDraft((current) => ({
          ...DEFAULT_DRAFT,
          ...current,
          ar: value.ar,
          od: value.od,
          cs: value.cs,
          hp: value.hp,
          end_time_ms: value.durationMs,
        }));
      })
      .catch((caught) => {
        if (!cancelled) setError(String(caught));
      });
    return () => {
      cancelled = true;
    };
  }, [client, resourceId]);
  useEffect(() => {
    let disposeTime: () => void = () => undefined;
    let disposeError: () => void = () => undefined;
    void desktopApi
      .onLiveRenderTime((state) => {
        if (!state.active) {
          setActive(false);
          activeRef.current = false;
          return;
        }
        setPlaying(state.playing);
        setDuration(state.durationMs);
        setTime(state.timeMs);
      })
      .then((fn) => {
        disposeTime = fn;
      });
    void desktopApi
      .onLiveRenderError((message) => {
        setActive(false);
        activeRef.current = false;
        setPlaying(false);
        setError(message);
      })
      .then((fn) => {
        disposeError = fn;
      });
    return () => {
      disposeTime();
      disposeError();
    };
  }, []);
  useEffect(
    () => () => {
      requestVersion.current += 1;
      dragHandle.current = null;
      activeRef.current = false;
      void desktopApi.liveRenderClose().catch(() => undefined);
    },
    [],
  );
  useEffect(() => {
    if (!active) return;
    const element = previewRef.current;
    if (!element) return;
    let frame = 0;
    let pending = false;
    let suppressed = false;
    const dialogOpen = () =>
      document.querySelector('[role="dialog"]') !== null ||
      document.body.hasAttribute("data-scroll-locked");
    const push = () => {
      pending = false;
      const box = element.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;
      void desktopApi
        .liveRenderMove({
          x: box.x * dpr,
          y: box.y * dpr,
          width: box.width * dpr,
          height: box.height * dpr,
          suppressed,
        })
        .catch(() => undefined);
    };
    const schedule = () => {
      if (pending) return;
      pending = true;
      frame = window.requestAnimationFrame(push);
    };
    const observer = new ResizeObserver(schedule);
    observer.observe(element);
    const detectDialog = () => {
      const next = dialogOpen();
      if (next !== suppressed) {
        suppressed = next;
        schedule();
      }
    };
    const mutation = new MutationObserver(detectDialog);
    mutation.observe(document.body, {
      childList: true,
      subtree: true,
      attributes: true,
    });
    window.addEventListener("resize", schedule);
    window.addEventListener("scroll", schedule, true);
    document.addEventListener("scroll", schedule, true);
    window.visualViewport?.addEventListener("resize", schedule);
    window.visualViewport?.addEventListener("scroll", schedule);
    detectDialog();
    push();
    return () => {
      window.cancelAnimationFrame(frame);
      observer.disconnect();
      mutation.disconnect();
      window.removeEventListener("resize", schedule);
      window.removeEventListener("scroll", schedule, true);
      document.removeEventListener("scroll", schedule, true);
      window.visualViewport?.removeEventListener("resize", schedule);
      window.visualViewport?.removeEventListener("scroll", schedule);
    };
  }, [active]);

  const effectiveRate =
    draft.bpm_locked && draft.target_bpm && timeline?.primaryBpm
      ? draft.target_bpm / timeline.primaryBpm
      : draft.rate;
  const effectiveAr =
    !draft.lock_ar &&
    draft.scale_ar &&
    timeline &&
    timeline.mode !== 1 &&
    timeline.mode !== 3
      ? scaledAr(timeline.ar, effectiveRate)
      : draft.ar;
  const effectiveOd =
    !draft.lock_od && draft.scale_od && timeline
      ? scaledOd(timeline.od, effectiveRate)
      : draft.od;
  const effectiveCs = draft.lock_cs ? draft.cs : (timeline?.cs ?? draft.cs);
  const effectiveHp = draft.lock_hp ? draft.hp : (timeline?.hp ?? draft.hp);
  const mapDuration = timeline?.durationMs ?? duration;
  const windowLength = clamp(
    draft.window_ms ?? 30_000,
    5_000,
    Math.max(5_000, mapDuration),
  );
  const start = clamp(
    draft.start_time_ms ?? 0,
    0,
    Math.max(0, mapDuration - windowLength),
  );
  const end = Math.min(mapDuration, start + windowLength);
  const strainValues = useMemo(() => {
    if (!timeline?.strainSeries.length) return [];
    const count = Math.max(
      ...timeline.strainSeries.map((series) => series.values.length),
      0,
    );
    return Array.from({ length: count }, (_, index) =>
      Math.max(
        ...timeline.strainSeries.map((series) => series.values[index] ?? 0),
        0,
      ),
    );
  }, [timeline]);
  const strainPeak = strainValues.length ? Math.max(...strainValues) : 0;
  const request = useMemo<ViewTrainerRequest>(
    () => ({
      client,
      resourceId,
      rate: effectiveRate,
      bpmLocked: draft.bpm_locked,
      targetBpm: draft.target_bpm,
      ar: effectiveAr,
      od: effectiveOd,
      cs: effectiveCs,
      hp: effectiveHp,
      scaleAr: draft.scale_ar,
      scaleOd: draft.scale_od,
      lockAr: draft.lock_ar,
      lockOd: draft.lock_od,
      lockCs: draft.lock_cs,
      lockHp: draft.lock_hp,
      noSpinners: draft.no_spinners,
      changePitch: draft.change_pitch,
      previewOnly: false,
      minBpm: draft.min_bpm,
      maxBpm: draft.max_bpm,
      startTimeMs: start || null,
      endTimeMs: end >= mapDuration - 1 ? null : end,
    }),
    [
      client,
      draft,
      effectiveAr,
      effectiveCs,
      effectiveHp,
      effectiveOd,
      effectiveRate,
      end,
      mapDuration,
      resourceId,
      start,
    ],
  );
  const rect = useCallback(() => {
    const box = previewRef.current?.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    return box
      ? {
          x: box.x * dpr,
          y: box.y * dpr,
          width: box.width * dpr,
          height: box.height * dpr,
        }
      : { x: 0, y: 0, width: 640, height: 360 };
  }, []);
  const openPreview = useCallback(
    (path: string, version = requestVersion.current) => {
      const run = async () => {
        if (activeRef.current) await desktopApi.liveRenderClose();
        if (version !== requestVersion.current) return false;
        await desktopApi.liveRenderOpen(
          path,
          "",
          {
            hud: false,
            storyboard: false,
            video: false,
            urBar: false,
            followPoints: true,
            keyOverlay: false,
            ppDisplay: false,
            bg: false,
            bgOpacity: 0,
            audio: true,
            audioOffset: 0,
            hitsounds: true,
            cursorSize: 1,
            skinPath: null,
            skinColours: false,
            avatarPath: null,
          },
          rect(),
        );
        if (version !== requestVersion.current) {
          await desktopApi.liveRenderClose().catch(() => undefined);
          return false;
        }
        activeRef.current = true;
        setActive(true);
        setTime(0);
        setPlaying(false);
        return true;
      };
      const queued = previewQueue.current.then(run, run);
      previewQueue.current = queued.then(
        () => undefined,
        () => undefined,
      );
      return queued;
    },
    [rect],
  );
  const generatePreview = useCallback(async () => {
    if (generatingRef.current) return;
    generatingRef.current = true;
    const version = ++requestVersion.current;
    activeRef.current = false;
    setActive(false);
    setPlaying(false);
    setTime(0);
    setGeneratedPath(null);
    setStagedDirectory(null);
    setImportedPath(null);
    setImporting(false);
    setGenerating(true);
    setError(null);
    await desktopApi.liveRenderClose().catch(() => undefined);
    try {
      const generated = await desktopApi.viewTrainerGenerate({
        ...request,
        previewOnly: false,
      });
      if (version !== requestVersion.current) return;
      setGeneratedPath(generated.beatmap_path);
      setStagedDirectory(generated.directory);
      await openPreview(generated.beatmap_path, version);
    } catch (caught) {
      if (version === requestVersion.current) setError(String(caught));
    } finally {
      generatingRef.current = false;
      if (version === requestVersion.current) setGenerating(false);
    }
  }, [openPreview, request]);
  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.isContentEditable)
      )
        return;
      if (event.key === " ") {
        event.preventDefault();
        if (activeRef.current)
          setPlaying((value) => {
            void (value
              ? desktopApi.liveRenderPause()
              : desktopApi.liveRenderPlay());
            return !value;
          });
        return;
      }
      if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
        event.preventDefault();
        const delta = event.key === "ArrowLeft" ? -1 : 1;
        const amount = event.shiftKey ? 30_000 : 5_000;
        const next = clamp(time + delta * amount, 0, duration);
        setTime(next);
        void desktopApi.liveRenderSeek(next);
        return;
      }
      if (event.key === "Enter" && event.ctrlKey) {
        event.preventDefault();
        void generatePreview();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [duration, generatePreview, time]);
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !timeline || !strainValues.length) return;
    const ratio = window.devicePixelRatio || 1;
    const width = canvas.clientWidth || 640;
    const height = canvas.clientHeight || 112;
    canvas.width = width * ratio;
    canvas.height = height * ratio;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
    ctx.clearRect(0, 0, width, height);
    const peak = Math.max(strainPeak, 0.0001);
    ctx.beginPath();
    strainValues.forEach((value, index) => {
      const x = (index / Math.max(1, strainValues.length - 1)) * width;
      const y = height - 6 - (value / peak) * (height - 14);
      if (index === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    });
    ctx.lineTo(width, height - 6);
    ctx.lineTo(0, height - 6);
    ctx.closePath();
    ctx.fillStyle = "rgba(165,243,252,.32)";
    ctx.fill();
    ctx.beginPath();
    strainValues.forEach((value, index) => {
      const x = (index / Math.max(1, strainValues.length - 1)) * width;
      const y = height - 6 - (value / peak) * (height - 14);
      if (index === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    });
    ctx.strokeStyle = "rgba(165,243,252,.95)";
    ctx.lineWidth = 1.5;
    ctx.stroke();
    const sectionStart = timeline.strainSectionStartTimeMs;
    const sectionLength = Math.max(1, timeline.strainSectionLengthMs);
    const curveDuration = Math.max(mapDuration, sectionStart + sectionLength);
    const x0 = clamp((start / curveDuration) * width, 0, width);
    const x1 = clamp((end / curveDuration) * width, 0, width);
    ctx.fillStyle = "rgba(244,114,182,.18)";
    ctx.fillRect(x0, 0, Math.max(2, x1 - x0), height);
    ctx.strokeStyle = "rgba(244,114,182,.95)";
    ctx.lineWidth = 2;
    ctx.strokeRect(x0, 1, Math.max(2, x1 - x0), height - 2);
    ctx.fillStyle = "rgba(251,113,133,.98)";
    ctx.fillRect(Math.max(0, x0 - 3), 0, 6, height);
    ctx.fillRect(Math.max(0, x1 - 3), 0, 6, height);
  }, [end, mapDuration, start, strainPeak, strainValues, timeline]);
  const handleStrainPointer = (
    event: React.PointerEvent<HTMLCanvasElement>,
  ) => {
    if (!timeline || (event.type === "pointermove" && !dragHandle.current))
      return;
    const box = event.currentTarget.getBoundingClientRect();
    const maxStart = Math.max(0, mapDuration - windowLength);
    const ratio = clamp(
      (event.clientX - box.left) / Math.max(1, box.width),
      0,
      1,
    );
    const value = ratio * mapDuration;
    if (event.type === "pointerdown") {
      const left = (start / Math.max(1, mapDuration)) * box.width;
      const right = (end / Math.max(1, mapDuration)) * box.width;
      const distanceToLeft = Math.abs(event.clientX - box.left - left);
      const distanceToRight = Math.abs(event.clientX - box.left - right);
      dragHandle.current =
        Math.min(distanceToLeft, distanceToRight) <= 14
          ? distanceToLeft <= distanceToRight
            ? "start"
            : "end"
          : "seek";
      event.currentTarget.setPointerCapture(event.pointerId);
    }
    if (dragHandle.current === "start") {
      const nextStart = clamp(value, 0, Math.max(0, end - 5_000));
      updateDraft({ start_time_ms: nextStart, end_time_ms: end });
    } else if (dragHandle.current === "end") {
      const nextEnd = clamp(
        value,
        Math.min(mapDuration, start + 5_000),
        mapDuration,
      );
      updateDraft({ window_ms: nextEnd - start, end_time_ms: nextEnd });
    } else {
      const nextStart = clamp(value - windowLength / 2, 0, maxStart);
      updateDraft({
        start_time_ms: nextStart,
        end_time_ms: nextStart + windowLength,
      });
    }
    if (event.type === "pointerup" || event.type === "pointercancel") {
      dragHandle.current = null;
      if (event.currentTarget.hasPointerCapture(event.pointerId))
        event.currentTarget.releasePointerCapture(event.pointerId);
    }
  };
  const findPeakWindow = useCallback(() => {
    if (!timeline || !strainValues.length) return;
    const sectionMs = Math.max(1, timeline.strainSectionLengthMs);
    const span = Math.max(1, Math.ceil(windowLength / sectionMs));
    let bestIndex = 0;
    let bestScore = -1;
    for (let index = 0; index < strainValues.length; index += 1) {
      let score = 0;
      for (
        let offset = 0;
        offset < span && index + offset < strainValues.length;
        offset += 1
      )
        score += strainValues[index + offset];
      if (score > bestScore) {
        bestScore = score;
        bestIndex = index;
      }
    }
    updateDraft({
      start_time_ms: clamp(
        timeline.strainSectionStartTimeMs + bestIndex * sectionMs,
        0,
        Math.max(0, mapDuration - windowLength),
      ),
    });
  }, [mapDuration, strainValues, timeline, updateDraft, windowLength]);
  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.isContentEditable)
      )
        return;
      if (event.key.toLowerCase() === "p") {
        event.preventDefault();
        findPeakWindow();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [findPeakWindow]);
  const stop = () => {
    activeRef.current = false;
    setActive(false);
    setPlaying(false);
    void desktopApi.liveRenderClose().catch(() => undefined);
  };

  if (!resourceId)
    return (
      <EmptyState
        icon={<Music4 className="size-6" />}
        title="选择一个本地谱面"
        description="从本地谱面进入 View Trainer。"
      />
    );
  return (
    <div className="space-y-5">
      <PageHeader
        eyebrow="View Trainer"
        title="谱面实时编辑器"
        description="快速调整训练参数，预览只写入 OPP 暂存区，确认后再导入 osu!。"
      />
      {error ? <Card className="p-4 text-sm text-red-200">{error}</Card> : null}
      {!timeline ? (
        <Card className="h-48 animate-pulse" />
      ) : (
        <div className="grid gap-5 xl:grid-cols-[minmax(0,1fr)_380px]">
          <div className="space-y-5">
            <Card className="p-5">
              <SectionTitle
                title="实时预览"
                description={`${timeline.objectCount.toLocaleString()} 个物件 · ${formatTime(timeline.durationMs)} · 实时背景音频`}
              />
              <div
                ref={previewRef}
                className="relative mt-4 aspect-video w-full overflow-hidden rounded-lg border border-white/10 bg-[#0e0e13]"
              >
                <div className="absolute inset-0 grid place-items-center text-sm text-slate-600">
                  {active ? "" : "准备预览…"}
                </div>
              </div>
              <div className="mt-4 flex flex-wrap items-center gap-2">
                {active ? (
                  <>
                    <Button
                      size="sm"
                      variant="primary"
                      onClick={() => {
                        setPlaying(!playing);
                        void (playing
                          ? desktopApi.liveRenderPause()
                          : desktopApi.liveRenderPlay());
                      }}
                    >
                      {playing ? (
                        <Pause className="size-4" />
                      ) : (
                        <Play className="size-4" />
                      )}
                      {playing ? "暂停" : "播放"}
                    </Button>
                    <Button size="sm" onClick={stop}>
                      <Square className="size-4" />
                      停止
                    </Button>
                    <span className="font-mono text-xs text-slate-400">
                      {formatTime(time)} / {formatTime(duration)}
                    </span>
                  </>
                ) : (
                  <Button
                    size="sm"
                    variant="primary"
                    loading={generating}
                    onClick={() => void generatePreview()}
                  >
                    <Play className="size-4" />
                    开始预览
                  </Button>
                )}
              </div>
              <input
                aria-label="预览进度"
                className="mt-3 w-full accent-cyan-400"
                type="range"
                min={0}
                max={Math.max(duration, 1)}
                step={10}
                value={Math.min(time, duration)}
                disabled={!active}
                onChange={(event) => {
                  const value = Number(event.target.value);
                  setTime(value);
                  void desktopApi.liveRenderSeek(value);
                }}
              />
            </Card>
            <Card className="p-5">
              <div className="flex items-start justify-between gap-3">
                <SectionTitle
                  title="Strain 难度窗口"
                  description="基于 rosu-pp 难度曲线选择训练区间。"
                />
                <Button size="sm" onClick={findPeakWindow}>
                  最高区间
                </Button>
              </div>
              <canvas
                ref={canvasRef}
                className="mt-5 h-28 w-full touch-none rounded-lg border border-white/10 bg-black/20"
                onPointerDown={handleStrainPointer}
                onPointerMove={handleStrainPointer}
                onPointerUp={handleStrainPointer}
                onPointerCancel={handleStrainPointer}
              />
              <div className="mt-4 grid gap-3 sm:grid-cols-[1fr_130px]">
                <label className="text-xs text-slate-400">
                  滑动窗口位置{" "}
                  <input
                    aria-label="滑动窗口位置"
                    className="mt-2 w-full accent-pink-300"
                    type="range"
                    min="0"
                    max={Math.max(0, mapDuration - windowLength)}
                    step="100"
                    value={start}
                    onChange={(event) =>
                      updateDraft({
                        start_time_ms: Number(event.target.value),
                        end_time_ms: Number(event.target.value) + windowLength,
                      })
                    }
                  />
                </label>
                <label className="text-xs text-slate-400">
                  窗口长度（秒）
                  <input
                    className="opp-input mt-1 w-full"
                    type="number"
                    min="5"
                    max={Math.max(5, Math.round(mapDuration / 1000))}
                    step="1"
                    value={Math.round(windowLength / 1000)}
                    onChange={(event) => {
                      const seconds = clamp(
                        Number(event.target.value),
                        5,
                        Math.max(5, mapDuration / 1000),
                      );
                      updateDraft({
                        window_ms: seconds * 1000,
                        start_time_ms: clamp(
                          start,
                          0,
                          Math.max(0, mapDuration - seconds * 1000),
                        ),
                        end_time_ms: clamp(
                          start + seconds * 1000,
                          1,
                          mapDuration,
                        ),
                      });
                    }}
                  />
                </label>
              </div>
              <div className="mt-3 flex flex-wrap gap-x-4 gap-y-1 text-xs text-slate-500">
                <span>
                  训练区间 {formatTime(start)} - {formatTime(end)}
                </span>
                <span>峰值 strain {strainPeak.toFixed(2)}</span>
                <span>
                  {timeline.strainSeries.length
                    ? timeline.strainSeries.length + " 条曲线"
                    : "暂无 strain 数据"}
                </span>
              </div>
            </Card>
          </div>
          <aside className="space-y-5">
            <Card className="p-5">
              <div className="flex items-center gap-3">
                <Gauge className="size-5 text-cyan-300" />
                <SectionTitle
                  title="速度与难度"
                  description="锁定值不会随倍率变化；AR/OD 可按 osu! 规则缩放。"
                />
              </div>
              <label className="mt-5 block text-xs text-slate-400">
                速度（{effectiveRate.toFixed(2)}x）
                <input
                  aria-label="速度"
                  className="mt-2 w-full accent-cyan-300"
                  type="range"
                  min="0.75"
                  max="2"
                  step="0.01"
                  value={effectiveRate}
                  disabled={draft.bpm_locked}
                  onChange={(event) =>
                    updateDraft({ rate: Number(event.target.value) })
                  }
                />
              </label>
              <div className="mt-5 grid grid-cols-2 gap-3">
                {(["ar", "od", "cs", "hp"] as const).map((key) => {
                  const lockKey = `lock_${key}` as
                    | "lock_ar"
                    | "lock_od"
                    | "lock_cs"
                    | "lock_hp";
                  const locked = draft[lockKey];
                  const value =
                    key === "ar"
                      ? effectiveAr
                      : key === "od"
                        ? effectiveOd
                        : key === "cs"
                          ? effectiveCs
                          : effectiveHp;
                  const automatic =
                    !locked &&
                    ((key === "ar" && draft.scale_ar) ||
                      (key === "od" && draft.scale_od) ||
                      key === "cs" ||
                      key === "hp");
                  return (
                    <label className="text-xs text-slate-400" key={key}>
                      {key.toUpperCase()}
                      <div className="mt-1 flex gap-1">
                        <input
                          className="opp-input min-w-0 flex-1"
                          max={key === "ar" || key === "od" ? 11 : 10}
                          min="0"
                          step="0.1"
                          type="number"
                          value={value.toFixed(1)}
                          disabled={automatic}
                          onChange={(event) =>
                            updateDraft({
                              [key]: clamp(
                                Number(event.target.value),
                                0,
                                key === "ar" || key === "od" ? 11 : 10,
                              ),
                            })
                          }
                        />
                        <ToggleButton
                          label={key.toUpperCase()}
                          locked={locked}
                          onClick={() => updateDraft({ [lockKey]: !locked })}
                        />
                      </div>
                    </label>
                  );
                })}
              </div>
              <div className="mt-4 grid grid-cols-2 gap-2 text-xs">
                <label className="flex items-center gap-2">
                  <input
                    checked={draft.scale_ar}
                    onChange={(event) =>
                      updateDraft({ scale_ar: event.target.checked })
                    }
                    type="checkbox"
                  />
                  随速缩放 AR
                </label>
                <label className="flex items-center gap-2">
                  <input
                    checked={draft.scale_od}
                    onChange={(event) =>
                      updateDraft({ scale_od: event.target.checked })
                    }
                    type="checkbox"
                  />
                  随速缩放 OD
                </label>
              </div>
            </Card>
            <Card className="p-5">
              <SectionTitle
                title="BPM 与训练开关"
                description="目标 BPM 锁定会自动换算 Rate；范围筛选按原谱面 timing point 判断。"
              />
              <div className="mt-5 grid grid-cols-2 gap-3">
                <label className="text-xs text-slate-400">
                  目标 BPM
                  {draft.bpm_locked ? (
                    <input
                      className="opp-input mt-1 w-full"
                      min="1"
                      max="1000"
                      step="1"
                      type="number"
                      value={
                        draft.target_bpm ?? Math.round(timeline.primaryBpm ?? 0)
                      }
                      onChange={(event) =>
                        updateDraft({
                          target_bpm: numberValue(event.target.value, null),
                        })
                      }
                    />
                  ) : (
                    <span className="mt-1 block text-slate-500">
                      未锁定 · 主 BPM {timeline.primaryBpm?.toFixed(0) ?? "-"}
                    </span>
                  )}
                </label>
                <label className="flex items-end gap-2 pb-1 text-xs text-slate-400">
                  <input
                    checked={draft.bpm_locked}
                    onChange={(event) =>
                      updateDraft({
                        bpm_locked: event.target.checked,
                        target_bpm: event.target.checked
                          ? (draft.target_bpm ?? timeline.primaryBpm)
                          : draft.target_bpm,
                      })
                    }
                    type="checkbox"
                  />
                  锁定目标 BPM
                </label>
                <label className="text-xs text-slate-400">
                  最低 BPM
                  <input
                    className="opp-input mt-1 w-full"
                    min="1"
                    type="number"
                    value={draft.min_bpm ?? ""}
                    onChange={(event) =>
                      updateDraft({
                        min_bpm: numberValue(event.target.value, null),
                      })
                    }
                  />
                </label>
                <label className="text-xs text-slate-400">
                  最高 BPM
                  <input
                    className="opp-input mt-1 w-full"
                    min="1"
                    type="number"
                    value={draft.max_bpm ?? ""}
                    onChange={(event) =>
                      updateDraft({
                        max_bpm: numberValue(event.target.value, null),
                      })
                    }
                  />
                </label>
              </div>
              <div className="mt-5 grid grid-cols-2 gap-2 text-xs">
                <label className="flex items-center gap-2">
                  <input
                    checked={draft.no_spinners}
                    onChange={(event) =>
                      updateDraft({ no_spinners: event.target.checked })
                    }
                    type="checkbox"
                  />
                  移除转盘
                </label>
                <label className="flex items-center gap-2">
                  <input
                    checked={draft.change_pitch}
                    onChange={(event) =>
                      updateDraft({ change_pitch: event.target.checked })
                    }
                    type="checkbox"
                  />
                  随速变调
                </label>
              </div>
            </Card>
            <div className="flex gap-2">
              <Button
                className="w-full"
                loading={generating}
                onClick={() => void generatePreview()}
                variant="primary"
              >
                <WandSparkles className="size-4" />
                重新生成预览
              </Button>
            </div>
            {generatedPath && stagedDirectory ? (
              <Button
                className="w-full"
                disabled={importing}
                loading={importing}
                onClick={() => {
                  setImporting(true);
                  setError(null);
                  void desktopApi
                    .viewTrainerImport(client, resourceId, stagedDirectory)
                    .then(setImportedPath)
                    .catch((caught) => setError(String(caught)))
                    .finally(() => setImporting(false));
                }}
              >
                <Download className="size-4" />
                {importing ? "正在导入…" : "导入到 osu!"}
              </Button>
            ) : null}
            {importedPath ? (
              <p className="break-all text-xs text-cyan-200">
                已导入：{importedPath}
              </p>
            ) : null}
          </aside>
        </div>
      )}
    </div>
  );
}
