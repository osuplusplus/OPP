import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { Area, AreaChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { Download, FolderOpen, ImageIcon, RefreshCw } from "lucide-react";
import { ErrorPanel } from "../../../shared/components/ErrorPanel";
import { Badge, Button, Card, SectionTitle } from "../../../shared/components/ui";
import { desktopApi } from "../../../shared/lib/tauri";
import type { BeatmapPreviewInspection, BeatmapPreviewResult } from "../../../shared/types/osu";
import { DifficultyIcon, ModIcon, modeMods } from "../../online-beatmaps/BeatmapVisuals";

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

export function BeatmapPreviewCard({ embeddedBid, embeddedBeatmapset }: { embeddedBid?: number; embeddedBeatmapset?: { beatmaps: Array<{ id: number; version: string; mode: string; difficulty_rating: number }> } } = {}) {
  const [searchParams] = useSearchParams();
  const linkedBid = embeddedBid ? String(embeddedBid) : searchParams.get("preview_bid") ?? "";
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
  const [mods, setMods] = useState<string[]>([]);
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
          index * strains.section_length_ms) /
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
    // The drag layer is inset to match the chart plot area. Use its own bounds
    // so the handles map to the same x coordinates as the visible selection.
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

  const beginHandleDrag = (event: React.PointerEvent<HTMLButtonElement>, mode: PreviewDragMode) => {
    event.preventDefault();
    event.stopPropagation();
    const layer = event.currentTarget.parentElement as HTMLDivElement | null;
    if (!layer) return;
    layer.setPointerCapture?.(event.pointerId);
    previewDragRef.current = {
      mode,
      pointerTime: previewTimeFromPointer(layer, event.clientX),
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
        ...(mods.length ? { mods } : {}),
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
      <SectionTitle title="谱面预览" description={embeddedBeatmapset ? "选择一个难度生成预览图" : "输入 Beatmap ID；std 选择 strain 区间生成 GIF，其他模式生成完整 PNG。"} />
    </div>

    {embeddedBeatmapset ? (
      <div className="mt-5">
        <h3 className="text-sm font-semibold text-white">选择难度</h3>
        <div className="mt-3 space-y-2">
          {embeddedBeatmapset.beatmaps
            .slice()
            .sort((left, right) => left.difficulty_rating - right.difficulty_rating)
            .map((beatmap) => (
              <button
                className={`group w-full rounded-2xl border p-4 text-left transition hover:border-cyan-300/30 hover:bg-cyan-300/[0.035] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-cyan-300/50 ${bid === String(beatmap.id) ? "border-cyan-300/40 bg-cyan-300/[0.07]" : "border-white/[0.06] bg-white/[0.025]"}`}
                key={beatmap.id}
                onClick={() => { setBid(String(beatmap.id)); void inspect(String(beatmap.id)); }}
                type="button"
              >
                <div className="flex items-center gap-3">
                  <DifficultyIcon mode={beatmap.mode as any} stars={beatmap.difficulty_rating} />
                  <div className="min-w-0 flex-1">
                    <p className="truncate font-medium text-slate-100">{beatmap.version}</p>
                    <p className="mt-1 text-[10px] uppercase tracking-wider text-slate-600">{beatmap.mode}</p>
                  </div>
                  <span className="text-sm font-mono text-cyan-200">{beatmap.difficulty_rating.toFixed(2)}★</span>
                </div>
              </button>
            ))}
        </div>
      </div>
    ) : (
      <form className="mt-5 flex flex-wrap items-end gap-3" onSubmit={(event) => { event.preventDefault(); void inspect(bid); }}>
        <label className="min-w-64 flex-1"><span className="mb-1.5 block text-xs text-slate-400">Beatmap ID</span><input aria-label="Beatmap ID" className="min-h-10 w-full rounded-xl border border-white/[0.09] bg-black/20 px-3 font-mono text-sm text-white outline-none focus:border-[var(--theme-primary)]" inputMode="numeric" onChange={(event) => setBid(event.target.value.replace(/\D/g, ""))} placeholder="例如 738063" value={bid} /></label>
        <Button loading={inspecting} type="submit" variant="primary"><RefreshCw className="size-4" />读取谱面</Button>
      </form>
    )}

    {error ? <div className="mt-4"><ErrorPanel error={error} onRetry={() => inspection ? void generate() : void inspect(bid)} /></div> : null}
    {notice ? <div className="mt-4 rounded-xl border border-emerald-300/20 bg-emerald-300/[0.08] px-4 py-3 text-sm text-emerald-100">{notice}</div> : null}

    {inspection ? <div className="mt-5 space-y-5">
      <div className="rounded-2xl border border-white/[0.08] bg-white/[0.025] p-4">
        <div className="flex flex-wrap items-center gap-2"><Badge tone="cyan">{previewModeLabels[inspection.ruleset]}</Badge><Badge>{previewTimestamp(lengthSeconds)}</Badge><span className="text-sm font-semibold text-white">{inspection.title_unicode || inspection.title} [{inspection.difficulty_name}]</span></div>
        <p className="mt-1 text-xs text-slate-400">{inspection.artist_unicode || inspection.artist} · mapped by {inspection.creator}</p>
      </div>
      <div className="flex flex-wrap gap-2">{modeMods[inspection.ruleset].map((mod) => <ModIcon key={mod} mod={mod} active={mods.includes(mod)} onClick={() => setMods((current) => current.includes(mod) ? current.filter((item) => item !== mod) : [...current, mod])} />)}</div>

      {inspection.ruleset === "osu" ? <div className="rounded-2xl border border-violet-300/15 bg-violet-300/[0.035] p-4">
        <div className="flex flex-wrap items-center justify-between gap-3"><div><h3 className="text-sm font-semibold text-white">选择 GIF 区间</h3><p className="mt-1 text-xs text-slate-400">拖动选中区域可整体平移，拖动两侧边界可调整 1–30 秒区间。</p></div><span className="font-mono text-sm text-violet-100">{previewTimestamp(startSeconds)} – {previewTimestamp(endSeconds)} · {(endSeconds - startSeconds).toFixed(1)}s</span></div>
        <div className="relative mt-4 h-44 w-full select-none">
          <ResponsiveContainer height="100%" width="100%"><AreaChart data={strainRows} margin={{ left: 0, right: 0, top: 8, bottom: 0 }}><defs><linearGradient id="preview-strain" x1="0" x2="0" y1="0" y2="1"><stop offset="0%" stopColor="#a78bfa" stopOpacity={0.55} /><stop offset="100%" stopColor="#a78bfa" stopOpacity={0.03} /></linearGradient></defs><CartesianGrid stroke="rgba(255,255,255,.05)" vertical={false} /><XAxis dataKey="time" domain={[0, lengthSeconds]} minTickGap={48} stroke="#64748b" tickFormatter={(value) => previewTimestamp(Number(value))} tickLine={false} type="number" /><YAxis hide domain={[0, "auto"]} /><Tooltip contentStyle={{ background: "#111827", border: "1px solid rgba(255,255,255,.1)", borderRadius: 12 }} formatter={(value) => [Number(value).toFixed(2), "Strain"]} labelFormatter={(value) => previewTimestamp(Number(value))} /><Area dataKey="strain" fill="url(#preview-strain)" isAnimationActive={false} stroke="#a78bfa" strokeWidth={1.5} type="monotone" /></AreaChart></ResponsiveContainer>
          <div
            aria-label="GIF 图表区间选择"
            className="absolute inset-x-0 bottom-7 top-2 z-10 touch-none"
            onPointerCancel={endPreviewDrag}
            onPointerDown={beginPreviewDrag}
            onPointerMove={movePreviewDrag}
            onPointerUp={endPreviewDrag}
          >
            <button
              aria-label="拖动 GIF 选中区间"
              className="absolute inset-y-0 z-10 cursor-grab border-y border-cyan-200/50 bg-cyan-300/[0.08] active:cursor-grabbing"
              data-preview-drag="move"
              style={{ left: `${(startSeconds / Math.max(lengthSeconds, 1)) * 100}%`, width: `${((endSeconds - startSeconds) / Math.max(lengthSeconds, 1)) * 100}%` }}
              type="button"
            />
            <button
              aria-label="拖动 GIF 开始边界"
              className="absolute inset-y-0 z-20 w-3 -translate-x-1/2 cursor-ew-resize rounded-sm border border-cyan-100/80 bg-cyan-300/70 shadow-[0_0_12px_rgba(103,232,249,.35)]"
              data-preview-drag="start"
              onPointerDown={(event) => beginHandleDrag(event, "start")}
              style={{ left: `${(startSeconds / Math.max(lengthSeconds, 1)) * 100}%` }}
              type="button"
            />
            <button
              aria-label="拖动 GIF 结束边界"
              className="absolute inset-y-0 z-20 w-3 -translate-x-1/2 cursor-ew-resize rounded-sm border border-violet-100/80 bg-violet-300/70 shadow-[0_0_12px_rgba(196,181,253,.35)]"
              data-preview-drag="end"
              onPointerDown={(event) => beginHandleDrag(event, "end")}
              style={{ left: `${(endSeconds / Math.max(lengthSeconds, 1)) * 100}%` }}
              type="button"
            />
          </div>
        </div>
        <div className="mt-3 flex items-center justify-between gap-3 text-[11px] text-slate-500">
          <span>开始 {previewTimestamp(startSeconds)}</span>
          <span className="text-center text-cyan-200/80">拖动选区移动，拖动两侧边界展开或收窄</span>
          <span>结束 {previewTimestamp(endSeconds)}</span>
        </div>
        <div className="sr-only" aria-hidden="false">
          <label>GIF 开始时间<input aria-label="GIF 开始时间" max={Math.max(0, lengthSeconds)} min="0" onChange={(event) => setStart(Number(event.target.value))} step="0.1" type="range" value={startSeconds} /></label>
          <label>GIF 结束时间<input aria-label="GIF 结束时间" max={Math.max(0, lengthSeconds)} min="0" onChange={(event) => setEnd(Number(event.target.value))} step="0.1" type="range" value={endSeconds} /></label>
        </div>
      </div> : <div className="rounded-xl border border-cyan-300/15 bg-cyan-300/[0.05] px-4 py-3 text-sm text-cyan-100">该模式将生成包含全部物件的 PNG 长图，无需选择区间。</div>}

      <Button disabled={inspection.ruleset === "osu" && lengthSeconds < 1} loading={generating} onClick={() => void generate()} variant="primary"><ImageIcon className="size-4" />生成{inspection.ruleset === "osu" ? " GIF" : " PNG"}</Button>

      {result && previewUrl ? <div className="rounded-2xl border border-white/[0.08] bg-black/20 p-4"><div className="mb-3 flex flex-wrap items-center justify-between gap-3"><div><h3 className="text-sm font-semibold text-white">生成结果</h3><p className="mt-1 font-mono text-xs text-slate-500">{result.file_name}</p></div><div className="flex gap-2"><Button loading={saving} onClick={() => void saveResult()} size="sm"><Download className="size-3.5" />另存为</Button><Button onClick={() => void desktopApi.openBeatmapPreviewOutput(result.output_path).catch(setError)} size="sm"><FolderOpen className="size-3.5" />打开所在文件夹</Button></div></div><div className="max-h-[560px] overflow-auto rounded-xl bg-[#080b12] p-3"><img alt="谱面预览生成结果" className="mx-auto max-w-full" src={previewUrl} /></div></div> : null}
    </div> : null}
  </Card>;

}
