import * as Dialog from "@radix-ui/react-dialog";
import { useState } from "react";
import {
  ExternalLink,
  Calculator,
  Headphones,
  Heart,
  LoaderCircle,
  Pause,
  ScanSearch,
  X,
} from "lucide-react";
import { Badge, Button, DataLine } from "../../shared/components/ui";
import { APP_TIME_ZONE, fullNumber } from "../../shared/lib/format";
import { desktopApi } from "../../shared/lib/tauri";
import type { OnlineBeatmapset, Ruleset } from "../../shared/types/osu";
import { useOnlineBeatmapsetDetail } from "./api";
import { DifficultyIcon, ModIcon, ModeIcon, modeMods } from "./BeatmapVisuals";
import {
  durationLabel,
  normalizePreviewUrl,
  starRange,
} from "./filters";

export function BeatmapsetDetailDialog({
  beatmapsetId,
  fallback,
  initialBeatmapId,
  onAddToCollection,
  onFindSimilar,
  playing,
  onClose,
  onPreview,
}: {
  beatmapsetId: number | null;
  fallback: OnlineBeatmapset | null;
  initialBeatmapId?: number | null;
  onAddToCollection: (beatmapset: OnlineBeatmapset) => void;
  onFindSimilar: (beatmapId: number, ruleset: Ruleset) => void;
  playing: boolean;
  onClose: () => void;
  onPreview: (beatmapset: OnlineBeatmapset) => void;
}) {
  const detailQuery = useOnlineBeatmapsetDetail(beatmapsetId);
  const beatmapset = detailQuery.data ?? fallback;
  const [selectedBeatmapId, setSelectedBeatmapId] = useState<number | null>(
    initialBeatmapId ?? null,
  );
  const [mods, setMods] = useState<string[]>([]);
  const [accuracy, setAccuracy] = useState("100");
  const [misses, setMisses] = useState("0");
  const [calculation, setCalculation] = useState<Awaited<ReturnType<typeof desktopApi.calculateBeatmapPp>> | null>(null);
  const [calculationError, setCalculationError] = useState<string | null>(null);
  const [showCalculator, setShowCalculator] = useState(false);
  const selectedMode = beatmapset?.beatmaps?.find((beatmap) => beatmap.id === selectedBeatmapId)?.mode ?? "osu";

  const calculate = async () => {
    if (!selectedBeatmapId) return;
    setCalculationError(null);
    try {
      setCalculation(await desktopApi.calculateBeatmapPp({
        beatmap_id: selectedBeatmapId,
        mods,
        accuracy: Number(accuracy),
        misses: Number(misses),
      }));
    } catch (error) {
      setCalculationError((error as { message?: string }).message ?? String(error));
    }
  };

  const toggleMod = (mod: string) => {
    setMods((current) => {
      if (current.includes(mod)) return current.filter((item) => item !== mod);
      const blocked = new Set<string>();
      if (mod === "DT") blocked.add("NC");
      if (mod === "NC") blocked.add("DT");
      if (mod === "EZ") blocked.add("HR");
      if (mod === "HR") blocked.add("EZ");
      if (mod === "HT") { blocked.add("DT"); blocked.add("NC"); }
      if (mod === "DT" || mod === "NC") blocked.add("HT");
      return [...current.filter((item) => !blocked.has(item)), mod];
    });
  };

  return (
    <Dialog.Root onOpenChange={(open) => !open && onClose()} open={beatmapsetId !== null}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-[80] bg-black/55 backdrop-blur-md" />
        <Dialog.Content className="beatmap-detail-dialog fixed left-1/2 top-1/2 z-[90] max-h-[min(720px,calc(100vh-2rem))] w-[min(720px,calc(100vw-2rem))] -translate-x-1/2 -translate-y-1/2 overflow-y-auto outline-none">
          {beatmapset ? (
            <>
              <div className="relative h-60 overflow-hidden bg-[#080b14]">
                {beatmapset.covers?.["cover@2x"] || beatmapset.covers?.cover ? (
                  <img
                    alt=""
                    className="absolute inset-0 size-full object-cover opacity-65"
                    src={beatmapset.covers["cover@2x"] ?? beatmapset.covers.cover}
                  />
                ) : null}
                <div className="absolute inset-0 bg-gradient-to-t from-[#0b101b] via-[#0b101b]/30 to-black/20" />
                <div className="absolute right-5 top-5 z-10 flex items-center gap-2">
                  <Button
                    aria-label="关闭详情"
                    className="order-2 bg-black/45 backdrop-blur"
                    onClick={onClose}
                    size="icon"
                  >
                    <X className="size-4" />
                  </Button>
                  <Button onClick={() => onAddToCollection(beatmapset)} variant="secondary"><Heart className="size-4" />加入收藏夹</Button>
                </div>
                <div className="absolute bottom-6 left-7 right-7">
                  <div className="flex gap-2">
                    <Badge tone="success">{beatmapset.status.toUpperCase()}</Badge>
                    <Badge>{beatmapset.beatmaps?.length ?? 0} 个难度</Badge>
                  </div>
                  <Dialog.Title className="mt-3 text-2xl font-semibold tracking-tight text-white">
                    {beatmapset.title}
                  </Dialog.Title>
                  <Dialog.Description className="mt-1 text-sm text-slate-300">
                    {beatmapset.artist} · mapped by {beatmapset.creator}
                  </Dialog.Description>
                </div>
              </div>

              <div className="p-7">
                <div className="mb-6 flex flex-wrap gap-2">
                  <Button
                    disabled={!normalizePreviewUrl(beatmapset.preview_url)}
                    onClick={() => onPreview(beatmapset)}
                    variant={playing ? "primary" : "secondary"}
                  >
                    {playing ? <Pause className="size-4" /> : <Headphones className="size-4" />}
                    {playing ? "暂停试听" : "试听预览"}
                  </Button>
                  <Button
                    onClick={() =>
                      desktopApi.openExternal(
                        `https://osu.ppy.sh/beatmapsets/${beatmapset.id}`,
                      )
                    }
                  >
                    <ExternalLink className="size-4" />
                    osu! 官网
                  </Button>
                </div>

                {detailQuery.isLoading && !detailQuery.data ? (
                  <div className="flex items-center gap-2 py-8 text-sm text-slate-500">
                    <LoaderCircle className="size-4 animate-spin" />
                    正在获取完整信息
                  </div>
                ) : null}

                <div className="grid grid-cols-2 gap-x-8 rounded-2xl border border-white/[0.065] bg-white/[0.025] px-5 py-2">
                  <DataLine label="谱面集 ID" value={beatmapset.id} />
                  <DataLine label="难度范围" value={starRange(beatmapset.beatmaps)} />
                  <DataLine label="BPM" value={beatmapset.bpm ? Math.round(beatmapset.bpm) : "—"} />
                  <DataLine label="游玩次数" value={fullNumber(beatmapset.play_count ?? 0)} />
                  <DataLine label="收藏数" value={fullNumber(beatmapset.favourite_count ?? 0)} />
                  <DataLine
                    label="Rank 日期"
                    value={
                      beatmapset.ranked_date
                        ? new Intl.DateTimeFormat("zh-CN", { timeZone: APP_TIME_ZONE }).format(new Date(beatmapset.ranked_date))
                        : "—"
                    }
                  />
                  <DataLine label="流派" value={beatmapset.genre?.name ?? "—"} />
                  <DataLine label="语言" value={beatmapset.language?.name ?? "—"} />
                </div>

                <div className="mt-7">
                  <h3 className="text-sm font-semibold text-white">难度列表</h3>
                  <div className="mt-3 space-y-2">
                    {(beatmapset.beatmaps ?? [])
                      .slice()
                      .sort(
                        (left, right) =>
                          left.difficulty_rating - right.difficulty_rating,
                      )
                      .map((beatmap) => (
                        <div
                          className={`group w-full rounded-2xl border p-4 text-left transition hover:border-cyan-300/30 hover:bg-cyan-300/[0.035] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-cyan-300/50 ${selectedBeatmapId === beatmap.id ? "border-cyan-300/40 bg-cyan-300/[0.07]" : "border-white/[0.06] bg-white/[0.025]"}`}
                          key={beatmap.id}
                          onClick={() => { setSelectedBeatmapId(beatmap.id); setCalculation(null); setShowCalculator(true); }}
                          onKeyDown={(event) => {
                            if (event.key === "Enter" || event.key === " ") {
                              event.preventDefault();
                              setSelectedBeatmapId(beatmap.id);
                              setCalculation(null);
                              setShowCalculator(true);
                            }
                          }}
                          role="button"
                          tabIndex={0}
                        >
                          <div className="flex items-center gap-3">
                            <div className="grid size-10 shrink-0 place-items-center rounded-xl border border-white/[0.08] bg-black/15 text-slate-300"><ModeIcon mode={beatmap.mode} /></div>
                            <DifficultyIcon mode={beatmap.mode} stars={beatmap.difficulty_rating} />
                            <Button
                              onClick={(event) => {
                                event.stopPropagation();
                                onFindSimilar(beatmap.id, beatmap.mode);
                              }}
                              size="sm"
                              variant="ghost"
                            >
                              <ScanSearch className="size-3.5" />
                              查找相似
                            </Button>
                            <div className="min-w-0 flex-1"><p className="truncate font-medium text-slate-100">{beatmap.version}</p><p className="mt-1 text-[10px] uppercase tracking-wider text-slate-600">{beatmap.mode}</p></div>
                            <div className="hidden text-right sm:block"><DifficultyIcon mode={beatmap.mode} stars={beatmap.difficulty_rating} /><p className="mt-1 text-[10px] text-slate-600">{durationLabel(beatmap.total_length)}</p></div>
                            <span className="grid size-9 shrink-0 place-items-center rounded-lg border border-cyan-300/20 text-cyan-200 transition group-hover:border-cyan-300/50 group-hover:bg-cyan-300/10" title="打开 PP 计算器"><Calculator className="size-4" /></span>
                          </div>
                          <div className="mt-4 grid grid-cols-3 gap-x-4 gap-y-2 border-t border-white/[0.055] pt-3 text-xs sm:grid-cols-6">
                            <span><b className="font-normal text-slate-600">BPM</b><strong className="ml-1.5 font-mono text-slate-300">{beatmap.bpm?.toFixed(0) ?? "—"}</strong></span>
                            <span><b className="font-normal text-slate-600">AR</b><strong className="ml-1.5 font-mono text-slate-300">{beatmap.ar?.toFixed(1) ?? "—"}</strong></span>
                            <span><b className="font-normal text-slate-600">OD</b><strong className="ml-1.5 font-mono text-slate-300">{beatmap.accuracy?.toFixed(1) ?? "—"}</strong></span>
                            <span><b className="font-normal text-slate-600">CS</b><strong className="ml-1.5 font-mono text-slate-300">{beatmap.cs?.toFixed(1) ?? "—"}</strong></span>
                            <span><b className="font-normal text-slate-600">HP</b><strong className="ml-1.5 font-mono text-slate-300">{beatmap.drain?.toFixed(1) ?? "—"}</strong></span>
                            <span><b className="font-normal text-slate-600">物件</b><strong className="ml-1.5 font-mono text-slate-300">{fullNumber((beatmap.count_circles ?? 0) + (beatmap.count_sliders ?? 0) + (beatmap.count_spinners ?? 0))}</strong></span>
                          </div>
                          <div className="mt-2 text-[11px] text-slate-500">通过 {fullNumber(beatmap.passcount ?? 0)} · 游玩 {fullNumber(beatmap.playcount ?? 0)}</div>
                        </div>
                      ))}
                  </div>
                </div>

                {showCalculator ? <section className="mt-7 border-t border-[var(--line-subtle)] pt-5">
                <div className="w-full">
                  <div className="flex items-center justify-between gap-3">
                    <div>
                      <h3 className="text-sm font-semibold text-white">单谱面难度 / PP</h3>
                      <p className="mt-1 text-[10px] text-slate-600">选择上方难度后从 Catboy 获取 .osu 文件计算</p>
                    </div>
                    <div className="flex gap-2"><Button onClick={() => setShowCalculator(false)} size="sm" variant="ghost">关闭</Button><Button disabled={!selectedBeatmapId} loading={false} onClick={calculate} size="sm">计算</Button></div>
                  </div>
          {calculation ? <div className="mt-4 rounded-xl border border-[var(--theme-primary-soft)] bg-[var(--theme-primary-muted)] p-5 text-center"><p className="text-xs uppercase tracking-[0.2em] text-[var(--theme-primary)]">Performance</p><p className="mt-1 text-5xl font-semibold tracking-tight text-white">{calculation.pp.toFixed(2)}<span className="ml-2 text-lg text-[var(--theme-primary)]">pp</span></p><div className="mt-4 grid grid-cols-3 gap-2 text-left"><DataLine label="Stars" value={<DifficultyIcon mode={selectedMode} stars={calculation.stars} />} /><DataLine label="Max PP" value={calculation.max_pp.toFixed(2)} /><DataLine label="Max Combo" value={calculation.max_combo} /></div></div> : <div className="mt-4 rounded-xl border border-dashed border-white/10 p-5 text-center text-sm text-slate-500">调整参数后点击“计算”，查看谱面性能结果</div>}
                  <div className="mt-4 grid grid-cols-2 gap-3">
                    <label className="text-xs text-slate-500">Accuracy %<input className="mt-1 w-full rounded-xl border border-white/[0.08] bg-black/20 px-3 py-2 text-sm text-slate-200 outline-none focus:border-cyan-300/40" max="100" min="0" onChange={(e) => setAccuracy(e.target.value)} type="number" value={accuracy} /></label>
                    <label className="text-xs text-slate-500">Misses<input className="mt-1 w-full rounded-xl border border-white/[0.08] bg-black/20 px-3 py-2 text-sm text-slate-200 outline-none focus:border-cyan-300/40" min="0" onChange={(e) => setMisses(e.target.value)} type="number" value={misses} /></label>
                  </div>
                  <div className="mt-2 flex flex-wrap gap-1.5">{[95, 97, 98, 99, 100].map((preset) => <button className="rounded-lg border border-white/[0.08] px-2.5 py-1 text-xs text-slate-400 hover:border-cyan-300/30 hover:text-cyan-100" key={preset} onClick={() => setAccuracy(String(preset))} type="button">{preset}%</button>)}</div>
                  <div className="mt-4 flex flex-wrap gap-2">
                    {modeMods[selectedMode].map((mod) => (
                      <ModIcon active={mods.includes(mod)} key={mod} mod={mod} onClick={() => toggleMod(mod)} />
                    ))}
                  </div>
                  {calculation ? <div className="mt-3 grid gap-1 text-left text-[10px] text-slate-500"><span>Star 算法：{calculation.star_algorithm} · {calculation.star_algorithm_date}</span><span>Performance 算法：{calculation.performance_algorithm} · {calculation.performance_algorithm_date}</span></div> : null}
                  {calculationError ? <p className="mt-3 text-xs text-amber-200">{calculationError}</p> : null}
                </div>
                </section> : null}

                {beatmapset.tags ? (
                  <div className="mt-7">
                    <h3 className="text-sm font-semibold text-white">标签</h3>
                    <p className="mt-2 text-xs leading-6 text-slate-500">
                      {beatmapset.tags}
                    </p>
                  </div>
                ) : null}

                {detailQuery.error ? (
                  <p className="mt-5 rounded-xl border border-amber-300/10 bg-amber-300/[0.05] px-4 py-3 text-xs text-amber-100">
                    完整详情暂时不可用，当前显示搜索结果中的信息。
                  </p>
                ) : null}
              </div>
            </>
          ) : (
            <div className="grid h-full place-items-center text-slate-500">
              <LoaderCircle className="size-5 animate-spin" />
            </div>
          )}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
