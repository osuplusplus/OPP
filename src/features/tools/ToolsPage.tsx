
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { Area, AreaChart, CartesianGrid, ReferenceArea, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { AlertTriangle, Database, Download, FileCog, FolderOpen, HardDrive, ImageIcon, Info, Keyboard, Link2, MonitorCog, RefreshCw, RotateCcw, Save, Square, Wrench } from "lucide-react";

import { useMode } from "../../app/ModeContext";
import { ClientSwitch } from "../../shared/components/ClientSwitch";
import { ErrorPanel } from "../../shared/components/ErrorPanel";
import { PageHeader } from "../../shared/components/PageHeader";
import { Badge, Button, Card, SectionTitle } from "../../shared/components/ui";
import { desktopApi, useCapabilities } from "../../shared/lib/tauri";

import type { DefaultFileClients, LazerDedupeProgress, LazerDedupeResult, LazerDiskUsage, LazerRealmReadResult, ManiaConversionItem, OsuClient } from "../../shared/types/osu";
import type { BeatmapPreviewInspection, BeatmapPreviewResult } from "../../shared/types/osu";

function formatByteSize(bytes: number) {
  const units = ["B", "K", "M", "G", "T"];
  let value = bytes;
  let index = 0;
  while (value >= 1024 && index < units.length - 1) { value /= 1024; index += 1; }
  return `${value.toFixed(value >= 100 || index === 0 ? 0 : 1)}${units[index]}`;
}

const previewModeLabels = { osu: "osu!standard", taiko: "osu!taiko", fruits: "osu!catch", mania: "osu!mania" } as const;

function previewTimestamp(seconds: number) {
  const safe = Math.max(0, seconds);
  const minutes = Math.floor(safe / 60);
  const rest = safe - minutes * 60;
  return `${minutes}:${rest.toFixed(1).padStart(4, "0")}`;
}

type PreviewDragMode = "move" | "start" | "end";

type PreviewDragState = {
  mode: PreviewDragMode;
  pointerTime: number;
  start: number;
  end: number;
};

function roundPreviewTime(value: number) {
  return Math.round(value * 10) / 10;
}

export function BeatmapPreviewCard() {
  const [searchParams] = useSearchParams();
  const linkedBid = searchParams.get("preview_bid") ?? "";
  const [bid, setBid] = useState(linkedBid);
  const [inspection, setInspection] = useState<BeatmapPreviewInspection | null>(null);
  const [startSeconds, setStartSeconds] = useState(0);
  const [endSeconds, setEndSeconds] = useState(10);
  const [inspecting, setInspecting] = useState(false);
  const [generating, setGenerating] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [result, setResult] = useState<BeatmapPreviewResult | null>(null);
  const [previewUrl, setPreviewUrl] = useState<string | null>(null);
  const previewDragRef = useRef<PreviewDragState | null>(null);

  useEffect(() => () => { if (previewUrl) URL.revokeObjectURL(previewUrl); }, [previewUrl]);

  const inspect = useCallback(async (rawBid: string) => {
    const numericBid = Number(rawBid.trim());
    if (!Number.isInteger(numericBid) || numericBid <= 0) {
      setError({ code: "INVALID_BEATMAP_ID", message: "请输入有效的正整数 Beatmap ID。" });
      return;
    }
    setInspecting(true);
    setError(null);
    setNotice(null);
    setInspection(null);
    setResult(null);
    setPreviewUrl(null);
    try {
      const inspected = await desktopApi.inspectBeatmapPreview(numericBid);
      const lengthSeconds = inspected.length_ms / 1_000;
      setInspection(inspected);
      setStartSeconds(0);
      setEndSeconds(Math.min(10, lengthSeconds));
    } catch (caught) {
      setError(caught);
    } finally {
      setInspecting(false);
    }
  }, []);

  useEffect(() => {
    if (!linkedBid) return;
    window.setTimeout(() => {
      const target = document.getElementById("beatmap-preview");
      if (typeof target?.scrollIntoView === "function") target.scrollIntoView({ behavior: "smooth", block: "center" });
      void inspect(linkedBid);
    }, 0);
  }, [inspect, linkedBid]);

  const strainRows = useMemo(() => {
    const strains = inspection?.strains;
    if (!strains) return [];
    const pointCount = Math.max(0, ...strains.series.map((series) => series.values.length));
    return Array.from({ length: pointCount }, (_, index) => ({
      time:
        ((strains.section_start_time_ms ?? strains.first_object_time_ms) +
          (index + 1) * strains.section_length_ms) /
        1_000,
      strain: strains.series.reduce((sum, series) => sum + (series.values[index] ?? 0), 0),
    }));
  }, [inspection]);

  const lengthSeconds = (inspection?.length_ms ?? 0) / 1_000;
  const setStart = (value: number) => {
    const next = Math.max(0, Math.min(value, endSeconds - 1));
    setStartSeconds(Math.max(next, endSeconds - 30));
  };
  const setEnd = (value: number) => {
    const next = Math.min(lengthSeconds, Math.max(value, startSeconds + 1));
    setEndSeconds(Math.min(next, startSeconds + 30));
  };

  const previewTimeFromPointer = (element: HTMLDivElement, clientX: number) => {
    const bounds = element.getBoundingClientRect();
    if (bounds.width <= 0 || lengthSeconds <= 0) return 0;
    return Math.max(0, Math.min(lengthSeconds, ((clientX - bounds.left) / bounds.width) * lengthSeconds));
  };

  const beginPreviewDrag = (event: React.PointerEvent<HTMLDivElement>) => {
    const target = event.target instanceof HTMLElement ? event.target.closest<HTMLElement>("[data-preview-drag]") : null;
    const mode = target?.dataset.previewDrag as PreviewDragMode | undefined;
    if (!mode) return;
    event.preventDefault();
    event.currentTarget.setPointerCapture?.(event.pointerId);
    previewDragRef.current = {
      mode,
      pointerTime: previewTimeFromPointer(event.currentTarget, event.clientX),
      start: startSeconds,
      end: endSeconds,
    };
  };

  const movePreviewDrag = (event: React.PointerEvent<HTMLDivElement>) => {
    const drag = previewDragRef.current;
    if (!drag) return;
    event.preventDefault();
    const pointerTime = previewTimeFromPointer(event.currentTarget, event.clientX);
    if (drag.mode === "move") {
      const duration = drag.end - drag.start;
      const nextStart = Math.max(0, Math.min(lengthSeconds - duration, drag.start + pointerTime - drag.pointerTime));
      setStartSeconds(roundPreviewTime(nextStart));
      setEndSeconds(roundPreviewTime(nextStart + duration));
      return;
    }
    if (drag.mode === "start") {
      setStartSeconds(roundPreviewTime(Math.max(0, drag.end - 30, Math.min(pointerTime, drag.end - 1))));
      return;
    }
    setEndSeconds(roundPreviewTime(Math.min(lengthSeconds, drag.start + 30, Math.max(pointerTime, drag.start + 1))));
  };

  const endPreviewDrag = (event: React.PointerEvent<HTMLDivElement>) => {
    if (!previewDragRef.current) return;
    previewDragRef.current = null;
    event.currentTarget.releasePointerCapture?.(event.pointerId);
  };

  const generate = async () => {
    if (!inspection) return;
    setGenerating(true);
    setError(null);
    setNotice(null);
    setResult(null);
    setPreviewUrl(null);
    try {
      const generated = await desktopApi.generateBeatmapPreview({
        bid: inspection.bid,
        start_seconds: inspection.ruleset === "osu" ? startSeconds : null,
        end_seconds: inspection.ruleset === "osu" ? endSeconds : null,
      });
      const bytes = await desktopApi.readBeatmapPreviewOutput(generated.output_path);
      setPreviewUrl(URL.createObjectURL(new Blob([bytes], { type: generated.mime_type })));
      setResult(generated);
      setNotice(`预览已生成：${generated.file_name}`);
    } catch (caught) {
      setError(caught);
    } finally {
      setGenerating(false);
    }
  };

  const saveResult = async () => {
    if (!result) return;
    const extension = result.mime_type === "image/gif" ? "gif" : "png";
    const destination = await desktopApi.chooseBeatmapPreviewDestination(result.file_name, extension);
    if (!destination) return;
    setSaving(true);
    setError(null);
    try {
      const saved = await desktopApi.saveBeatmapPreviewOutput(result.output_path, destination);
      setNotice(`预览已保存到 ${saved}`);
    } catch (caught) {
      setError(caught);
    } finally {
      setSaving(false);
    }
  };

  return <Card className="scroll-mt-8 p-6" id="beatmap-preview">
    <div className="flex items-start gap-4">
      <div className="grid size-11 shrink-0 place-items-center rounded-2xl border border-[var(--theme-primary-soft)] bg-[var(--theme-primary-muted)] text-[var(--theme-primary)]"><ImageIcon className="size-5" /></div>
      <SectionTitle title="谱面预览" description="输入 Beatmap ID；std 选择 strain 区间生成 GIF，其他模式生成完整 PNG。" />
    </div>
    <form className="mt-5 flex flex-wrap items-end gap-3" onSubmit={(event) => { event.preventDefault(); void inspect(bid); }}>
      <label className="min-w-64 flex-1"><span className="mb-1.5 block text-xs text-slate-400">Beatmap ID</span><input aria-label="Beatmap ID" className="min-h-10 w-full rounded-xl border border-white/[0.09] bg-black/20 px-3 font-mono text-sm text-white outline-none focus:border-[var(--theme-primary)]" inputMode="numeric" onChange={(event) => setBid(event.target.value.replace(/\D/g, ""))} placeholder="例如 738063" value={bid} /></label>
      <Button loading={inspecting} type="submit" variant="primary"><RefreshCw className="size-4" />读取谱面</Button>
    </form>

    {error ? <div className="mt-4"><ErrorPanel error={error} onRetry={() => inspection ? void generate() : void inspect(bid)} /></div> : null}
    {notice ? <div className="mt-4 rounded-xl border border-emerald-300/20 bg-emerald-300/[0.08] px-4 py-3 text-sm text-emerald-100">{notice}</div> : null}

    {inspection ? <div className="mt-5 space-y-5">
      <div className="rounded-2xl border border-white/[0.08] bg-white/[0.025] p-4">
        <div className="flex flex-wrap items-center gap-2"><Badge tone="cyan">{previewModeLabels[inspection.ruleset]}</Badge><Badge>{previewTimestamp(lengthSeconds)}</Badge><span className="text-sm font-semibold text-white">{inspection.title_unicode || inspection.title} [{inspection.difficulty_name}]</span></div>
        <p className="mt-1 text-xs text-slate-400">{inspection.artist_unicode || inspection.artist} · mapped by {inspection.creator}</p>
      </div>

      {inspection.ruleset === "osu" ? <div className="rounded-2xl border border-violet-300/15 bg-violet-300/[0.035] p-4">
        <div className="flex flex-wrap items-center justify-between gap-3"><div><h3 className="text-sm font-semibold text-white">选择 GIF 区间</h3><p className="mt-1 text-xs text-slate-400">拖动选中区域可整体平移，拖动两侧边界可调整 1–30 秒区间。</p></div><span className="font-mono text-sm text-violet-100">{previewTimestamp(startSeconds)} – {previewTimestamp(endSeconds)} · {(endSeconds - startSeconds).toFixed(1)}s</span></div>
        <div className="relative mt-4 h-44 w-full select-none">
          <ResponsiveContainer height="100%" width="100%"><AreaChart data={strainRows} margin={{ left: 0, right: 8, top: 8, bottom: 0 }}><defs><linearGradient id="preview-strain" x1="0" x2="0" y1="0" y2="1"><stop offset="0%" stopColor="#a78bfa" stopOpacity={0.55} /><stop offset="100%" stopColor="#a78bfa" stopOpacity={0.03} /></linearGradient></defs><CartesianGrid stroke="rgba(255,255,255,.05)" vertical={false} /><XAxis dataKey="time" domain={[0, lengthSeconds]} minTickGap={48} stroke="#64748b" tickFormatter={(value) => previewTimestamp(Number(value))} tickLine={false} type="number" /><YAxis hide domain={[0, "auto"]} /><Tooltip contentStyle={{ background: "#111827", border: "1px solid rgba(255,255,255,.1)", borderRadius: 12 }} formatter={(value) => [Number(value).toFixed(2), "Strain"]} labelFormatter={(value) => previewTimestamp(Number(value))} /><ReferenceArea fill="#22d3ee" fillOpacity={0.13} stroke="#67e8f9" strokeOpacity={0.45} x1={startSeconds} x2={endSeconds} /><Area dataKey="strain" fill="url(#preview-strain)" isAnimationActive={false} stroke="#a78bfa" strokeWidth={1.5} type="monotone" /></AreaChart></ResponsiveContainer>
          <div
            aria-label="GIF 图表区间选择"
            className="absolute inset-x-2 bottom-7 top-2 z-10 touch-none"
            onPointerCancel={endPreviewDrag}
            onPointerDown={beginPreviewDrag}
            onPointerMove={movePreviewDrag}
            onPointerUp={endPreviewDrag}
          >
            <button
              aria-label="拖动 GIF 选中区间"
              className="absolute inset-y-0 cursor-grab border-y border-cyan-200/50 bg-cyan-300/[0.08] active:cursor-grabbing"
              data-preview-drag="move"
              style={{ left: `${(startSeconds / Math.max(lengthSeconds, 1)) * 100}%`, width: `${((endSeconds - startSeconds) / Math.max(lengthSeconds, 1)) * 100}%` }}
              type="button"
            />
            <button
              aria-label="拖动 GIF 开始边界"
              className="absolute inset-y-0 w-3 -translate-x-1/2 cursor-ew-resize rounded-sm border border-cyan-100/80 bg-cyan-300/70 shadow-[0_0_12px_rgba(103,232,249,.35)]"
              data-preview-drag="start"
              style={{ left: `${(startSeconds / Math.max(lengthSeconds, 1)) * 100}%` }}
              type="button"
            />
            <button
              aria-label="拖动 GIF 结束边界"
              className="absolute inset-y-0 w-3 -translate-x-1/2 cursor-ew-resize rounded-sm border border-violet-100/80 bg-violet-300/70 shadow-[0_0_12px_rgba(196,181,253,.35)]"
              data-preview-drag="end"
              style={{ left: `${(endSeconds / Math.max(lengthSeconds, 1)) * 100}%` }}
              type="button"
            />
          </div>
        </div>
        <div className="relative mt-3 h-9">
          <input aria-label="GIF 开始时间" className="pointer-events-none absolute inset-x-0 top-1 w-full accent-cyan-300 [&::-webkit-slider-thumb]:pointer-events-auto" max={Math.max(0, lengthSeconds)} min="0" onChange={(event) => setStart(Number(event.target.value))} step="0.1" type="range" value={startSeconds} />
          <input aria-label="GIF 结束时间" className="pointer-events-none absolute inset-x-0 top-1 w-full accent-violet-300 [&::-webkit-slider-thumb]:pointer-events-auto" max={Math.max(0, lengthSeconds)} min="0" onChange={(event) => setEnd(Number(event.target.value))} step="0.1" type="range" value={endSeconds} />
        </div>
      </div> : <div className="rounded-xl border border-cyan-300/15 bg-cyan-300/[0.05] px-4 py-3 text-sm text-cyan-100">该模式将生成包含全部物件的 PNG 长图，无需选择区间。</div>}

      <Button disabled={inspection.ruleset === "osu" && lengthSeconds < 1} loading={generating} onClick={() => void generate()} variant="primary"><ImageIcon className="size-4" />生成{inspection.ruleset === "osu" ? " GIF" : " PNG"}</Button>

      {result && previewUrl ? <div className="rounded-2xl border border-white/[0.08] bg-black/20 p-4"><div className="mb-3 flex flex-wrap items-center justify-between gap-3"><div><h3 className="text-sm font-semibold text-white">生成结果</h3><p className="mt-1 font-mono text-xs text-slate-500">{result.file_name}</p></div><div className="flex gap-2"><Button loading={saving} onClick={() => void saveResult()} size="sm"><Download className="size-3.5" />另存为</Button><Button onClick={() => void desktopApi.openBeatmapPreviewOutput(result.output_path).catch(setError)} size="sm"><FolderOpen className="size-3.5" />打开所在文件夹</Button></div></div><div className="max-h-[560px] overflow-auto rounded-xl bg-[#080b12] p-3"><img alt="谱面预览生成结果" className="mx-auto max-w-full" src={previewUrl} /></div></div> : null}
    </div> : null}
  </Card>;

}

function ManiaConverterCard() {
  const [busy, setBusy] = useState(false);
  const [items, setItems] = useState<ManiaConversionItem[]>([]);
  const [error, setError] = useState<unknown>(null);
  const convert = async () => { const paths = await desktopApi.chooseManiaBeatmaps(); if (!paths.length) return; setBusy(true); setError(null); try { setItems((await desktopApi.convertManiaBeatmaps(paths)).items); } catch (value) { setError(value); } finally { setBusy(false); } };
  return <Card className="p-6"><div className="flex items-start gap-4"><div className="grid size-11 shrink-0 place-items-center rounded-2xl border border-[var(--theme-primary-soft)] bg-[var(--theme-primary-muted)] text-[var(--theme-primary)]"><RefreshCw className="size-5" /></div><SectionTitle title="Malody 谱面转换" description="选择一个或多个 .mcz 文件，转换后的 .osz 将保存在原文件目录。已有同名输出会安全跳过。" /></div><div className="mt-5"><Button loading={busy} onClick={() => void convert()}><FolderOpen className="size-4" />选择 .mcz 文件并转换</Button></div>{error ? <div className="mt-4"><ErrorPanel error={error} /></div> : null}{items.length ? <div className="mt-5 space-y-2">{items.map((item) => <div className="rounded-xl border border-white/[0.09] bg-white/[0.035] px-4 py-3" key={item.input}><div className="flex items-center justify-between gap-3"><span className="min-w-0 truncate text-sm font-medium text-slate-100">{item.input}</span><Badge tone={item.status === "completed" ? "success" : item.status === "skipped" ? "warning" : "pink"}>{item.status === "completed" ? "已完成" : item.status === "skipped" ? "已跳过" : "失败"}</Badge></div><p className="mt-1 truncate text-xs text-slate-300">{item.output ?? item.message}</p></div>)}</div> : null}</Card>;
}

function DisplayGammaCard() {
  const [gamma, setGamma] = useState(1);
  const [applying, setApplying] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const apply = async (value: number) => { setApplying(true); setError(null); try { await desktopApi.setDisplayGamma(value); } catch (caught) { setError(caught); } finally { setApplying(false); } };
  const resetGamma = () => { setGamma(1); void apply(1); };
  return <Card className="p-6"><div className="flex items-start gap-4"><div className="grid size-11 shrink-0 place-items-center rounded-2xl border border-[var(--theme-primary-soft)] bg-[var(--theme-primary-muted)] text-[var(--theme-primary)]"><MonitorCog className="size-5" /></div><SectionTitle title="显示器伽马" description="直接使用 Windows 显示接口调整主显示器的伽马。1.00 为默认值，数值越高会提亮中间调。" /></div><div className="mt-6 flex items-center gap-4"><input aria-label="显示器伽马" className="h-2 w-full cursor-pointer accent-[var(--theme-primary)]" type="range" min="0.5" max="2.5" step="0.01" value={gamma} onChange={(event) => setGamma(Number(event.target.value))} onKeyUp={() => void apply(gamma)} onPointerUp={() => void apply(gamma)} /><output className="w-12 shrink-0 text-right text-sm font-semibold tabular-nums text-white">{gamma.toFixed(2)}</output><Button disabled={applying || gamma === 1} loading={applying} onClick={resetGamma} size="sm"><RotateCcw className="size-3.5" />复原</Button></div><p className="mt-3 text-xs leading-5 text-slate-400">松开滑块后应用，避免显示驱动积压请求；关闭或重启显示驱动后，Windows 可能会恢复系统默认伽马。</p>{error ? <div className="mt-4"><ErrorPanel error={error} /></div> : null}</Card>;
}

export function LegacySpeedTestCard() {
  const [active, setActive] = useState(false);
  const [binding, setBinding] = useState(false);
  const [testKeys, setTestKeys] = useState<string[]>(["KeyZ", "KeyX"]);
  const [presses, setPresses] = useState(0);
  const [duration, setDuration] = useState(10);
  const [remaining, setRemaining] = useState(10);
  const [result, setResult] = useState<number | null>(null);
  const startedAt = useRef(0);
  const pressesRef = useRef(0);
  useEffect(() => { if (!active) return; const onKeyDown = (event: KeyboardEvent) => { if (!event.repeat && event.code !== "Escape" && testKeys.includes(event.code)) { event.preventDefault(); pressesRef.current += 1; setPresses(pressesRef.current); } }; window.addEventListener("keydown", onKeyDown); const interval = window.setInterval(() => { const next = Math.max(0, duration - (performance.now() - startedAt.current) / 1000); setRemaining(next); if (next === 0) { window.clearInterval(interval); setActive(false); setResult(pressesRef.current / duration); } }, 50); return () => { window.removeEventListener("keydown", onKeyDown); window.clearInterval(interval); }; }, [active, duration, testKeys]);
  useEffect(() => { if (!binding) return; const onKeyDown = (event: KeyboardEvent) => { if (event.key === "Escape") { setBinding(false); return; } if (event.repeat || ["Control", "Alt", "Shift", "Meta"].includes(event.key)) return; event.preventDefault(); setTestKeys((current) => current.includes(event.code) ? current.filter((key) => key !== event.code) : [...current, event.code]); }; window.addEventListener("keydown", onKeyDown); return () => window.removeEventListener("keydown", onKeyDown); }, [binding]);
  const start = () => { startedAt.current = performance.now(); pressesRef.current = 0; setPresses(0); setRemaining(duration); setResult(null); setActive(true); };
  const bpm = result == null ? null : result * 15;
  return <Card className="p-6"><div className="flex items-start gap-4"><div className="grid size-11 shrink-0 place-items-center rounded-2xl border border-[var(--theme-primary-soft)] bg-[var(--theme-primary-muted)] text-[var(--theme-primary)]"><Keyboard className="size-5" /></div><SectionTitle title="手速测试" description="连续按任意按键（按住不算），按所选时间计算 KPS，并转换为每分钟 BPM。" /></div><div className="mt-5 flex items-center gap-3"><span className="text-xs text-slate-500">测试时长</span>{[5, 10, 15, 30].map((value) => <button className={`rounded-lg border px-3 py-1.5 text-xs font-semibold ${duration === value ? "border-[var(--theme-primary)] bg-[var(--theme-primary-muted)] text-white" : "border-white/[0.08] text-slate-400"}`} disabled={active} key={value} onClick={() => { setDuration(value); setRemaining(value); setResult(null); }} type="button">{value}s</button>)}</div><div className="mt-4 rounded-2xl border border-white/[0.08] bg-black/[0.12] p-5"><div className="flex items-end justify-between gap-5"><div><p className="text-xs uppercase tracking-[0.16em] text-slate-500">{active ? "正在计时" : result == null ? "准备开始" : "本次成绩"}</p><p className="mt-1 text-3xl font-semibold tabular-nums text-white">{active ? presses : result == null ? "—" : `${result.toFixed(1)} KPS`}</p>{bpm != null ? <p className="mt-1 text-sm font-semibold text-pink-200">≈ {bpm.toFixed(0)} BPM</p> : null}</div><div className="text-right"><p className="text-xs text-slate-500">剩余时间</p><p className="mt-1 text-xl font-semibold tabular-nums text-[var(--theme-primary-light)]">{remaining.toFixed(1)}s</p></div></div><div className="mt-4 h-2 overflow-hidden rounded-full bg-white/[0.08]"><div className="h-full rounded-full bg-[var(--theme-primary)] transition-[width]" style={{ width: `${((duration - remaining) / duration) * 100}%` }} /></div><Button className="mt-5" disabled={active} onClick={start} variant="primary">{result == null ? `开始 ${duration} 秒测试` : "再测一次"}</Button></div></Card>;
}

function SpeedTestCard() {
  return <><BeatmapPreviewCard /><SpeedTestContent /></>;
}

function SpeedTestContent() {
  const [active, setActive] = useState(false);
  const [armed, setArmed] = useState(false);
  const [binding, setBinding] = useState(false);
  const [testKeys, setTestKeys] = useState<string[]>(["KeyZ", "KeyX"]);
  const [presses, setPresses] = useState(0);
  const [duration, setDuration] = useState(10);
  const [remaining, setRemaining] = useState(10);
  const [result, setResult] = useState<number | null>(null);
  const [terminated, setTerminated] = useState(false);
  const [liveKps, setLiveKps] = useState(0);
  const [peakKps, setPeakKps] = useState(0);
  const [buckets, setBuckets] = useState<number[]>([]);
  const startedAt = useRef(0);
  const pressTimesRef = useRef<number[]>([]);

  useEffect(() => {
    if (!active) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (!event.repeat && testKeys.includes(event.code)) {
        event.preventDefault();
        pressTimesRef.current.push(performance.now());
        setPresses(pressTimesRef.current.length);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    const interval = window.setInterval(() => {
      const now = performance.now();
      const elapsed = (now - startedAt.current) / 1000;
      const next = Math.max(0, duration - elapsed);
      setRemaining(next);
      const times = pressTimesRef.current;
      // 实时 KPS：最近 1 秒滚动窗口内的按键数。
      const rolling = times.filter((time) => now - time <= 1000).length;
      setLiveKps(rolling);
      setPeakKps((current) => Math.max(current, rolling));
      // 每秒一档的按键直方图，随测试实时更新。
      const seconds = Math.max(1, Math.ceil(Math.min(elapsed + 0.05, duration)));
      const counts = Array.from({ length: seconds }, () => 0);
      for (const time of times) {
        counts[Math.min(seconds - 1, Math.floor((time - startedAt.current) / 1000))] += 1;
      }
      setBuckets(counts);
      if (next === 0) {
        window.clearInterval(interval);
        setActive(false);
        setResult(times.length / duration);
      }
    }, 50);
    return () => { window.removeEventListener("keydown", onKeyDown); window.clearInterval(interval); };
  }, [active, duration, testKeys]);

  useEffect(() => {
    if (!binding) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") { setBinding(false); return; }
      if (event.repeat || ["Control", "Alt", "Shift", "Meta"].includes(event.key)) return;
      event.preventDefault();
      setTestKeys((current) => current.includes(event.code) ? current.filter((key) => key !== event.code) : [...current, event.code]);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [binding]);

  const resetTest = () => {
    pressTimesRef.current = [];
    setPresses(0);
    setRemaining(duration);
    setResult(null);
    setTerminated(false);
    setLiveKps(0);
    setPeakKps(0);
    setBuckets([]);
  };
  const armTest = () => {
    resetTest();
    setArmed(true);
  };
  const terminate = () => { setTerminated(true); setActive(false); };

  useEffect(() => {
    if (!armed) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.repeat || !testKeys.includes(event.code)) return;
      event.preventDefault();
      const firstPressAt = performance.now();
      startedAt.current = firstPressAt;
      pressTimesRef.current = [firstPressAt];
      setPresses(1);
      setArmed(false);
      setActive(true);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [armed, testKeys]);

  const bpm = active ? liveKps * 15 : result == null ? null : result * 15;
  const chartMax = Math.max(1, ...buckets);
  // 折线图：x 轴按测试时长铺满，每秒一档的按键数归一化到 0-100。
  const linePoints = buckets.map((count, index) => `${(index / Math.max(1, duration - 1)) * 100},${100 - (count / chartMax) * 100}`).join(" ");
  const lineEndX = ((buckets.length - 1) / Math.max(1, duration - 1)) * 100;
  const status = active ? "测试中" : armed ? "等待测试按键" : terminated ? "已终止（不计成绩）" : result == null ? "准备开始" : "本次成绩";

  return <Card className="p-6">
    <div className="flex items-start gap-4"><div className="grid size-11 shrink-0 place-items-center rounded-2xl border border-[var(--theme-primary-soft)] bg-[var(--theme-primary-muted)] text-[var(--theme-primary)]"><Keyboard className="size-5" /></div><SectionTitle title="手速测试" description="仅统计绑定的测试按键；实时 KPS 取最近 1 秒滚动窗口，BPM 按四分之一拍换算。" /></div>
    <div className="mt-5 flex flex-wrap items-center gap-3"><span className="text-xs text-slate-500">测试时长</span>{[5, 10, 15, 30].map((value) => <button className={`rounded-lg border px-3 py-1.5 text-xs font-semibold ${duration === value ? "border-[var(--theme-primary)] bg-[var(--theme-primary-muted)] text-white" : "border-white/[0.08] text-slate-400"}`} disabled={active || armed} key={value} onClick={() => { setDuration(value); setRemaining(value); setResult(null); setTerminated(false); }} type="button">{value}s</button>)}</div>
    <div className="mt-3 flex flex-wrap items-center gap-2"><Button disabled={active || armed} onClick={() => setBinding((current) => !current)} size="sm" variant="secondary">{binding ? "完成绑定" : "绑定测试按键"}</Button><span className="font-mono text-xs text-cyan-200">{testKeys.length ? testKeys.join(" + ").replace(/Key/g, "") : "未绑定"}</span></div>
    {binding ? <p className="mt-2 text-xs text-amber-200">按一个键以添加或移除；按 Esc 结束绑定。</p> : null}
    <div className="mt-4 rounded-2xl border border-white/[0.08] bg-black/[0.12] p-5">
      <div className="flex items-end justify-between gap-5">
        <div>
          <p className="text-xs uppercase tracking-[0.16em] text-slate-500">{status}</p>
          <p className="mt-1 text-3xl font-semibold tabular-nums text-white">{active || terminated ? `${liveKps.toFixed(1)} KPS` : result == null ? "—" : `${result.toFixed(1)} KPS`}</p>
          {bpm != null ? <p className="mt-1 text-sm font-semibold text-pink-200">≈ {bpm.toFixed(0)} BPM（四分之一拍）</p> : null}
        </div>
        <div className="text-right">
          <p className="text-xs text-slate-500">剩余时间</p>
          <p className="mt-1 text-xl font-semibold tabular-nums text-[var(--theme-primary-light)]">{remaining.toFixed(1)}s</p>
          <p className="mt-1 text-xs text-slate-500">峰值 {peakKps.toFixed(1)} KPS · 已按 {presses} 次</p>
        </div>
      </div>
      <div className="mt-4 h-2 overflow-hidden rounded-full bg-white/[0.08]"><div className="h-full rounded-full bg-[var(--theme-primary)] transition-[width]" style={{ width: `${((duration - remaining) / duration) * 100}%` }} /></div>
      {buckets.length ? <div className="mt-4">
        <p className="text-xs text-slate-500">每秒按键数</p>
        <svg className="mt-2 h-24 w-full" preserveAspectRatio="none" viewBox="0 0 100 100">
          {buckets.length >= 2 ? <>
            <polygon fill="var(--theme-primary)" fillOpacity="0.12" points={`0,100 ${linePoints} ${lineEndX},100`} />
            <polyline fill="none" points={linePoints} stroke="var(--theme-primary)" strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" vectorEffect="non-scaling-stroke" />
          </> : null}
        </svg>
      </div> : null}
      <div className="mt-5 flex flex-wrap gap-3">
        {active
          ? <Button onClick={terminate} variant="danger"><Square className="size-4" />终止测试</Button>
          : armed
            ? <><Button disabled variant="primary">按 {testKeys.join(" / ").replace(/Key/g, "")} 开始计时</Button><Button onClick={() => setArmed(false)} variant="ghost">取消等待</Button></>
            : <Button disabled={!testKeys.length || binding} onClick={armTest} variant="primary">{result == null && !terminated ? `开始 ${duration} 秒测试` : "再测一次"}</Button>}
      </div>
    </div>
  </Card>;
}

function FileAssociationCard({ title, description, value, saving, onSave }: { title: string; description: string; value: OsuClient; saving: boolean; onSave: (client: OsuClient) => void }) {
  return <div className="rounded-2xl border border-white/[0.07] bg-white/[0.025] p-5"><div className="flex items-start justify-between gap-3"><div><h3 className="font-semibold text-white">{title}</h3><p className="mt-1 text-xs leading-5 text-slate-400">{description}</p></div><Badge tone={value === "stable" ? "pink" : "cyan"}>{value === "stable" ? "Stable" : "Lazer"}</Badge></div><div className="mt-5 flex flex-wrap items-center justify-between gap-3"><ClientSwitch value={value} onChange={onSave} /><Button loading={saving} onClick={() => onSave(value)} size="sm"><Save className="size-3.5" />保存设置</Button></div></div>;
}

export function ToolsPage() {
  const { client } = useMode();
  const capabilities = useCapabilities();
  const [defaults, setDefaults] = useState<DefaultFileClients>({ beatmap: client, skin: client });
  const [error, setError] = useState<unknown>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [saving, setSaving] = useState<"beatmap" | "skin" | null>(null);
  useEffect(() => { void desktopApi.getDefaultFileClients().then(setDefaults).catch(setError); }, []);
  const save = async (kind: "beatmap" | "skin", target: OsuClient) => { setSaving(kind); setNotice(null); setError(null); try { await desktopApi.setDefaultFileClient(kind, target); setDefaults((current) => ({ ...current, [kind]: target })); setNotice(`${kind === "beatmap" ? "谱面" : "Skin"} 默认打开端已设为 ${target === "stable" ? "Stable" : "Lazer"}`); } catch (caught) { setError(caught); } finally { setSaving(null); } };
  return <><PageHeader eyebrow="Tools" title="工具集合" description="集中放置 osu! 日常使用的小工具与系统集成设置。" actions={<Badge tone="cyan"><Wrench className="size-3.5" />实用工具</Badge>} />{error ? <div className="mb-5"><ErrorPanel error={error} onRetry={() => void desktopApi.getDefaultFileClients().then(setDefaults).catch(setError)} /></div> : null}{notice ? <div className="mb-5 rounded-xl border border-emerald-300/20 bg-emerald-300/[0.08] px-4 py-3 text-sm text-emerald-100">{notice}</div> : null}<div className="space-y-5"><SpeedTestCard />{capabilities.data?.file_association ? (<Card className="p-6"><div className="flex items-start gap-4"><div className="grid size-11 shrink-0 place-items-center rounded-2xl border border-[var(--theme-primary-soft)] bg-[var(--theme-primary-muted)] text-[var(--theme-primary)]"><FileCog className="size-5" /></div><SectionTitle title="文件默认打开端" description="设置 Windows 双击 .osz 谱面包和 .osk Skin 文件时使用的 osu! 客户端。" /></div><div className="mt-6 grid gap-4 lg:grid-cols-2"><FileAssociationCard title="谱面包文件 (.osz)" description="双击 .osz 文件时直接交给选中的客户端读取。" value={defaults.beatmap} saving={saving === "beatmap"} onSave={(target) => void save("beatmap", target)} /><FileAssociationCard title="Skin 文件 (.osk)" description="双击 .osk 文件时使用选中的客户端导入或读取。" value={defaults.skin} saving={saving === "skin"} onSave={(target) => void save("skin", target)} /></div><div className="mt-5 flex items-start gap-2 rounded-xl border border-amber-300/15 bg-amber-300/[0.06] p-3 text-sm leading-5 text-amber-100"><Info className="mt-0.5 size-4 shrink-0" />请先在设置中确认 Stable 或 Lazer 的游戏目录。</div></Card>) : null}{capabilities.data?.display_gamma ? <DisplayGammaCard /> : null}<ManiaConverterCard /><LazerDiskUsageCard /><LazerRealmReadCard /><LazerDedupeCard /></div></>;
}

function LazerDiskUsageCard() {
  const [data, setData] = useState<LazerDiskUsage | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const scan = async () => { setBusy(true); setError(null); try { setData(await desktopApi.getLazerDiskUsage()); } catch (value) { setError(value); } finally { setBusy(false); } };
  return <Card className="p-6"><div className="flex items-start gap-4"><div className="grid size-11 shrink-0 place-items-center rounded-2xl border border-[var(--theme-primary-soft)] bg-[var(--theme-primary-muted)] text-[var(--theme-primary)]"><Database className="size-5" /></div><SectionTitle title="osu!lazer 占用统计" description="按 storage.ini 的 FullPath 定位数据目录，统计含硬链接的总大小与排除硬链接后的实际占用。" /></div><div className="mt-5"><Button loading={busy} onClick={() => void scan()}><FolderOpen className="size-4" />{data ? "重新统计" : "开始统计"}</Button></div>{data ? <div className="mt-5 space-y-3"><p className="truncate font-mono text-xs text-slate-400">{data.path}</p><div className="grid gap-3 sm:grid-cols-3"><div className="rounded-xl border border-white/[0.08] bg-white/[0.03] p-4"><p className="text-xs text-slate-500">总大小</p><p className="mt-1 text-lg font-semibold tabular-nums text-white">{formatByteSize(data.total_size)}</p></div><div className="rounded-xl border border-white/[0.08] bg-white/[0.03] p-4"><p className="text-xs text-slate-500">实际占用</p><p className="mt-1 text-lg font-semibold tabular-nums text-[var(--theme-primary-light)]">{formatByteSize(data.unique_size)}</p></div><div className="rounded-xl border border-white/[0.08] bg-white/[0.03] p-4"><p className="text-xs text-slate-500">文件数</p><p className="mt-1 text-lg font-semibold tabular-nums text-white">{data.file_count.toLocaleString()}</p></div></div></div> : null}{error ? <div className="mt-4"><ErrorPanel error={error} /></div> : null}</Card>;
}

function LazerRealmReadCard() {
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<LazerRealmReadResult | null>(null);
  const [error, setError] = useState<unknown>(null);
  const run = async () => { setBusy(true); setError(null); try { setResult(await desktopApi.readLazerRealmBeatmapSets()); } catch (caught) { setError(caught); } finally { setBusy(false); } };
  const preview = result?.beatmap_sets.slice(0, 12) ?? [];
  return <Card className="p-6">
    <div className="flex items-start gap-4">
      <div className="grid size-11 shrink-0 place-items-center rounded-2xl border border-[var(--theme-primary-soft)] bg-[var(--theme-primary-muted)] text-[var(--theme-primary)]"><Database className="size-5" /></div>
      <SectionTitle title="Lazer Realm 读取测试" description="通过本地 realm-db-reader 直接解析 client.realm（只读快照，不影响游戏运行），验证谱面集归属、原始文件名与内容哈希的读取链路，为后续 osz 导出做准备。" />
    </div>
    <div className="mt-5"><Button loading={busy} onClick={() => void run()}><Database className="size-4" />{result ? "重新读取" : "读取 client.realm"}</Button></div>
    <p className="mt-3 text-xs leading-5 text-slate-500">库文件较大时全量解析可能需要数十秒，读取过程中请耐心等待。</p>
    {result ? <div className="mt-5 space-y-3">
      <p className="truncate font-mono text-xs text-slate-400" title={result.realm_path}>{result.realm_path}</p>
      <div className="grid gap-3 sm:grid-cols-2">
        <div className="rounded-xl border border-white/[0.08] bg-white/[0.03] p-4"><p className="text-xs text-slate-500">数据表</p><p className="mt-1 text-lg font-semibold tabular-nums text-white">{result.table_count.toLocaleString()}</p></div>
        <div className="rounded-xl border border-[var(--theme-primary-soft)] bg-[var(--theme-primary-muted)]/40 p-4"><p className="text-xs text-slate-500">谱面集（不含软删除）</p><p className="mt-1 text-lg font-semibold tabular-nums text-[var(--theme-primary-light)]">{result.beatmap_set_count.toLocaleString()}</p></div>
      </div>
      {preview.length ? <div className="rounded-xl border border-white/[0.08] bg-white/[0.03] p-4">
        <p className="text-xs text-slate-400">前 {preview.length} 个谱面集：</p>
        <div className="mt-2 space-y-1.5">
          {preview.map((set) => <div className="flex items-center justify-between gap-3 text-xs" key={set.id}>
            <span className="min-w-0 truncate text-slate-200">{set.artist} - {set.title} <span className="text-slate-500">({set.creator})</span></span>
            <span className="shrink-0 tabular-nums text-slate-500">{set.beatmap_count} 难度 · {set.files.length} 文件</span>
          </div>)}
        </div>
      </div> : null}
    </div> : null}
    {error ? <div className="mt-4"><ErrorPanel error={error} /></div> : null}
  </Card>;
}

const lazerDedupePhaseLabels: Record<string, string> = {
  "scan-lazer": "扫描 lazer 文件存储",
  "scan-stable": "扫描 stable 目录",
  hash: "计算 SHA-256 并匹配",
  link: "创建硬链接",
};

function LazerDedupeCard() {
  const [acknowledged, setAcknowledged] = useState(false);
  const [busy, setBusy] = useState<"preview" | "apply" | null>(null);
  const [progress, setProgress] = useState<LazerDedupeProgress | null>(null);
  const [result, setResult] = useState<LazerDedupeResult | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [error, setError] = useState<unknown>(null);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void desktopApi.onLazerDedupeProgress((value) => { if (!disposed) setProgress(value); }).then((dispose) => { if (disposed) dispose(); else unlisten = dispose; });
    return () => { disposed = true; unlisten?.(); };
  }, []);

  const run = async (dryRun: boolean) => {
    setBusy(dryRun ? "preview" : "apply");
    setError(null);
    setProgress(null);
    try {
      setResult(await desktopApi.dedupeLazerFiles(dryRun));
      if (!dryRun) setConfirming(false);
    } catch (caught) {
      setError(caught);
    } finally {
      setBusy(null);
      setProgress(null);
    }
  };

  return <Card className="p-6">
    <div className="flex items-start gap-4">
      <div className="grid size-11 shrink-0 place-items-center rounded-2xl border border-[var(--theme-primary-soft)] bg-[var(--theme-primary-muted)] text-[var(--theme-primary)]"><HardDrive className="size-5" /></div>
      <SectionTitle title="压缩 Lazer 空间" description="lazer 以内容 SHA-256 为文件名存储文件。按大小分组对比 stable 谱面目录（Songs）文件的哈希，把内容完全一致的 lazer 副本替换为指向 stable 的硬链接，不修改任何文件内容。" />
    </div>
    <div className="mt-5 rounded-xl border border-amber-300/20 bg-amber-300/[0.06] p-4">
      <p className="flex items-center gap-2 text-sm font-semibold text-amber-100"><AlertTriangle className="size-4 shrink-0" />使用前请阅读风险提示</p>
      <ul className="mt-2 space-y-1.5 text-xs leading-5 text-amber-100/80">
        <li>· 硬链接要求两端位于同一磁盘分区，且文件系统支持硬链接（NTFS / ext4 / btrfs 等；FAT32 不支持）。跨分区或不支持的文件会自动跳过</li>
        <li>· 硬链接共享同一份数据，只占用一份空间：osu! 更新资源均为整文件替换，正常使用不受影响；删除任一端文件也不影响另一端</li>
        <li>· 创建硬链接后如果需要修改 stable 的谱面文件，请使用 osu! 客户端进行编辑，请不要手动修改</li>
        <li>· 如果真的需要手动修改文件，请删除源文件后创建新的写入，这样会断开硬链接，否则会造成该文件在 lazer 哈希值错误而报错</li>
        <li>· 执行前请关闭 osu!</li>
      </ul>
      <label className="mt-3 flex cursor-pointer items-center gap-2 text-xs font-medium text-amber-100">
        <input checked={acknowledged} className="size-3.5 accent-[var(--theme-primary)]" onChange={(event) => setAcknowledged(event.target.checked)} type="checkbox" />
        我已了解上述风险，确认 lazer 与 stable 目录配置正确
      </label>
    </div>
    <div className="mt-5 flex flex-wrap items-center gap-3">
      <Button disabled={!acknowledged || busy !== null} loading={busy === "preview"} onClick={() => void run(true)}><FolderOpen className="size-4" />{result ? "重新扫描可释放空间" : "扫描可释放空间"}</Button>
      {busy ? <Button onClick={() => void desktopApi.cancelLazerDedupe()} variant="secondary"><Square className="size-4" />取消</Button> : null}
    </div>
    {busy && progress ? <div className="mt-4 rounded-xl border border-white/[0.08] bg-white/[0.03] p-4">
      <div className="flex items-center justify-between gap-3 text-xs text-slate-400">
        <span>{lazerDedupePhaseLabels[progress.phase] ?? progress.phase}</span>
        <span className="tabular-nums">{progress.total > 0 ? `${progress.processed.toLocaleString()} / ${progress.total.toLocaleString()}` : `${progress.processed.toLocaleString()} 个文件`}</span>
      </div>
      <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-white/[0.08]"><div className="h-full rounded-full bg-[var(--theme-primary)] transition-[width]" style={{ width: `${progress.percent}%` }} /></div>
    </div> : null}
    {result ? <div className="mt-5 space-y-3">
      <p className="truncate font-mono text-xs text-slate-400" title={result.lazer_files_root}>{result.lazer_files_root}{result.stable_roots.length ? ` ⟵ ${result.stable_roots.join("、")}` : ""}</p>
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <div className="rounded-xl border border-white/[0.08] bg-white/[0.03] p-4"><p className="text-xs text-slate-500">lazer 文件</p><p className="mt-1 text-lg font-semibold tabular-nums text-white">{result.lazer_file_count.toLocaleString()} · {formatByteSize(result.lazer_total_size)}</p></div>
        <div className="rounded-xl border border-white/[0.08] bg-white/[0.03] p-4"><p className="text-xs text-slate-500">已是硬链接</p><p className="mt-1 text-lg font-semibold tabular-nums text-slate-300">{result.already_linked_count.toLocaleString()} · {formatByteSize(result.already_linked_size)}</p></div>
        <div className="rounded-xl border border-white/[0.08] bg-white/[0.03] p-4"><p className="text-xs text-slate-500">参与哈希对比</p><p className="mt-1 text-lg font-semibold tabular-nums text-slate-300">{result.hashed_stable_count.toLocaleString()}</p></div>
        <div className="rounded-xl border border-[var(--theme-primary-soft)] bg-[var(--theme-primary-muted)]/40 p-4">
          <p className="text-xs text-slate-500">{result.dry_run ? "可释放" : "已释放"}</p>
          <p className="mt-1 text-lg font-semibold tabular-nums text-[var(--theme-primary-light)]">{result.dry_run ? `${result.candidate_count.toLocaleString()} 个 · ${formatByteSize(result.reclaimable_size)}` : `${result.linked_count.toLocaleString()} 个 · ${formatByteSize(result.linked_size)}`}</p>
        </div>
      </div>
      {result.cancelled ? <p className="text-xs text-amber-200">任务已取消，以上为部分结果。</p> : null}
      {result.skipped_cross_volume_count > 0 ? <p className="text-xs text-slate-400">{result.skipped_cross_volume_count.toLocaleString()} 个文件（{formatByteSize(result.skipped_cross_volume_size)}）与 stable 不在同一分区，已跳过。</p> : null}
      {result.failed_count > 0 ? <div className="rounded-xl border border-white/[0.08] bg-white/[0.03] p-4">
        <p className="text-xs text-slate-400">{result.failed_count.toLocaleString()} 个文件处理失败（仅展示前 {result.failed.length} 条）：</p>
        <div className="mt-2 max-h-32 space-y-1 overflow-y-auto">
          {result.failed.map((failure) => <p className="truncate font-mono text-xs text-slate-500" key={failure.path} title={`${failure.path}：${failure.message}`}>{failure.path} — {failure.message}</p>)}
        </div>
      </div> : null}
      {!result.dry_run ? <p className="text-xs text-emerald-200">完成：{result.linked_count.toLocaleString()} 个文件已替换为硬链接，实际释放约 {formatByteSize(result.linked_size)}。可启动 lazer 验证内容正常。</p> : null}
    </div> : null}
    {result?.dry_run && !busy && result.candidate_count > 0 ? (
      confirming ? <div className="mt-5 rounded-xl border border-rose-400/25 bg-rose-400/[0.07] p-4">
        <p className="text-sm leading-6 text-rose-100">即将把 <span className="font-semibold">{result.candidate_count.toLocaleString()}</span> 个 lazer 文件替换为指向 stable 的硬链接，预计释放 <span className="font-semibold">{formatByteSize(result.reclaimable_size)}</span>。请确认已关闭 osu!。</p>
        <div className="mt-3 flex flex-wrap gap-3">
          <Button loading={busy === "apply"} onClick={() => void run(false)} variant="danger"><Link2 className="size-4" />确认执行</Button>
          <Button disabled={busy !== null} onClick={() => setConfirming(false)} variant="secondary">取消</Button>
        </div>
      </div> : <div className="mt-5">
        <Button disabled={!acknowledged} onClick={() => setConfirming(true)} variant="primary"><Link2 className="size-4" />创建硬链接并释放空间</Button>
      </div>
    ) : null}
    {result?.dry_run && !busy && result.candidate_count === 0 ? <p className="mt-5 text-xs text-slate-400">没有发现可与 stable 合并的重复文件。</p> : null}
    {error ? <div className="mt-4"><ErrorPanel error={error} /></div> : null}
  </Card>;
}
