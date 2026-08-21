import { useQuery } from "@tanstack/react-query";
import { ArrowRight, Download, Headphones, Heart, Pause } from "lucide-react";

import { Badge, Button, Card } from "../../shared/components/ui";
import { DifficultyIcon } from "../../shared/components/DifficultyIcon";
import { desktopApi, isTauri } from "../../shared/lib/tauri";
import type { AnySimilarityBeatmap, AnySimilarityResult, ManiaModeFamily } from "../../shared/types/osu";

const maniaFamilyLabels: Record<ManiaModeFamily, string> = {
  rc: "RC",
  hb: "HB",
  mix: "Mix",
  ln: "LN",
};

function durationLabel(seconds: number) {
  const rounded = Math.max(0, Math.round(seconds));
  return `${Math.floor(rounded / 60)}:${String(rounded % 60).padStart(2, "0")}`;
}

function osuTone(value: number, scale = 10) {
  const normalized = value / scale;
  if (normalized <= 0.125) return "metric-tone-0";
  if (normalized <= 0.2) return "metric-tone-1";
  if (normalized <= 0.25) return "metric-tone-2";
  if (normalized <= 0.35) return "metric-tone-3";
  if (normalized <= 0.4) return "metric-tone-4";
  if (normalized <= 0.5) return "metric-tone-5";
  if (normalized <= 0.65) return "metric-tone-6";
  return "metric-tone-7";
}

function Metric({
  label,
  value,
  tone,
}: {
  label: string;
  value: string;
  tone: string;
}) {
  return (
    <div className="rounded-md border border-white/[0.06] bg-black/[0.12] px-3 py-2.5">
      <span className="block text-[11px] font-medium uppercase tracking-wide text-slate-500">{label}</span>
      <strong className={`metric-value ${tone} mt-0.5 block text-base`}>{value}</strong>
    </div>
  );
}

export function SimilarityResultCard({
  result,
  recommendedBy,
  selected,
  onSelect,
  onOpen,
  onDownload,
  onAddToCollection,
  onPreview,
  previewLoading,
  playing,
  downloading,
  downloadDisabled,
}: {
  result: AnySimilarityResult;
  recommendedBy?: AnySimilarityBeatmap;
  selected: boolean;
  onSelect: () => void;
  onOpen: () => void;
  onDownload: () => void;
  onAddToCollection: () => void;
  onPreview: () => void;
  previewLoading: boolean;
  playing: boolean;
  downloading: boolean;
  downloadDisabled: boolean;
}) {
  const beatmapset = useQuery({
    queryKey: ["similarity-result-beatmapset", result.ruleset, result.beatmapset_id],
    queryFn: () => desktopApi.getOnlineBeatmapset(result.beatmapset_id),
    enabled: isTauri(),
    staleTime: Infinity,
    retry: 1,
  });
  const cover = beatmapset.data?.covers?.card ?? beatmapset.data?.covers?.list ?? beatmapset.data?.covers?.cover;

  return (
    <Card
      className={`opp-interactive-surface theme-beatmap-card relative cursor-pointer overflow-hidden p-5 ${
        selected
          ? "selected-mask"
          : ""
      }`}
      onClick={onSelect}
      role="button"
      tabIndex={0}
    >
      {cover ? <img alt="" className="theme-beatmap-background pointer-events-none absolute inset-0 size-full object-cover opacity-[0.16]" src={cover} /> : null}
      <div className="relative flex items-start gap-4">
        <DifficultyIcon
          className="bg-[#0a0f1a]/85 shadow-lg backdrop-blur-sm"
          mode={result.ruleset}
          stars={result.ruleset === "osu" ? result.star_rating : null}
        />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <Badge className="normal-case text-xs tracking-normal" tone="cyan">难度 · {result.version}</Badge>
            <span className="truncate text-sm text-slate-500">#{result.beatmap_id}</span>
          </div>
          <h3 className="mt-2 truncate text-base font-semibold text-white">{result.artist} - {result.title}</h3>
          <p className="mt-1 truncate text-sm text-slate-400">[{result.version}] · mapped by {result.creator}</p>
          {recommendedBy ? (
            <p className="mt-2 truncate text-sm text-cyan-200/90">
              由 {recommendedBy.artist} - {recommendedBy.title} [{recommendedBy.version}] 推荐
            </p>
          ) : null}

          {result.ruleset === "mania" ? (
            <>
              <div className="mt-3 grid grid-cols-4 gap-2">
                <Metric label="键数" tone={osuTone(result.key_count, 10)} value={`${result.key_count}K`} />
                <Metric label="键型" tone={osuTone(result.style.chord_rate)} value={maniaFamilyLabels[result.family]} />
                <Metric label="主模式" tone={osuTone(result.style.stream)} value={result.pattern} />
                <Metric label="难度分位" tone={osuTone(result.difficulty_percentile, 1)} value={`${Math.round(result.difficulty_percentile * 100)}%`} />
                <Metric label="BPM" tone={osuTone(result.base.bpm, 300)} value={Math.round(result.base.bpm).toString()} />
                <Metric label="有效长度" tone={osuTone(result.base.active_length_seconds, 360)} value={durationLabel(result.base.active_length_seconds)} />
                <Metric label="平均 NPS" tone={osuTone(result.base.avg_nps, 15)} value={result.base.avg_nps.toFixed(2)} />
                <Metric label="LN 比例" tone={osuTone(result.style.ln_note_ratio, 1)} value={`${Math.round(result.style.ln_note_ratio * 100)}%`} />
              </div>
              <div aria-label="Mania 距离分量" className="mt-2 flex flex-wrap gap-x-3 gap-y-1 rounded-md border border-white/[0.06] bg-black/[0.12] px-3 py-2 font-mono text-[10px] text-slate-400">
                <span className="text-slate-300">总距 {result.final_distance.toFixed(4)}</span>
                <span>强度 {result.distance_components.skill.toFixed(3)}</span>
                <span>键型 {result.distance_components.pattern.toFixed(3)}</span>
                <span>结构 {result.distance_components.structure.toFixed(3)}</span>
                <span>分位 {result.distance_components.difficulty.toFixed(3)}</span>
                <span>上下文 {result.distance_components.context.toFixed(3)}</span>
              </div>
            </>
          ) : (
            <div className="mt-3 grid grid-cols-4 gap-2">
              <Metric label="AR" tone={osuTone(result.base.ar)} value={result.base.ar.toFixed(1)} />
              <Metric label="CS" tone={osuTone(result.base.cs)} value={result.base.cs.toFixed(1)} />
              <Metric label="OD" tone={osuTone(result.base.od)} value={result.base.od.toFixed(1)} />
              <Metric label="HP" tone={osuTone(result.base.hp)} value={result.base.hp.toFixed(1)} />
              <Metric label="BPM" tone={osuTone(result.base.bpm, 300)} value={Math.round(result.base.bpm).toString()} />
              <Metric label="长度" tone={osuTone(result.base.length_seconds, 360)} value={durationLabel(result.base.length_seconds)} />
              <Metric label="密度" tone={osuTone(result.base.object_density)} value={result.base.object_density.toFixed(2)} />
              <Metric label="物件" tone={osuTone(result.base.object_count, 1600)} value={Math.round(result.base.object_count).toString()} />
            </div>
          )}
        </div>
        <div className="flex shrink-0 items-center gap-1">
          <Button aria-label="加入收藏夹" onClick={(event) => { event.stopPropagation(); onAddToCollection(); }} size="icon" variant="ghost"><Heart className="size-4" /></Button>
          <Button aria-label={playing ? "暂停试听" : "试听"} loading={previewLoading} onClick={(event) => { event.stopPropagation(); onPreview(); }} size="icon" variant={playing ? "primary" : "ghost"}>
            {playing ? <Pause className="size-4" /> : <Headphones className="size-4" />}
          </Button>
          <Button aria-label={`快捷下载 ${result.artist} - ${result.title}`} disabled={downloadDisabled} loading={downloading} onClick={(event) => { event.stopPropagation(); onDownload(); }} size="icon" variant="ghost">
            <Download className="size-4" />
          </Button>
          <Button aria-label="在在线谱面中查看" onClick={(event) => { event.stopPropagation(); onOpen(); }} size="icon" variant="ghost">
            <ArrowRight className="size-4" />
          </Button>
        </div>
      </div>
    </Card>
  );
}
