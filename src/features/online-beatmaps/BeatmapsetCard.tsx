import { Check, Download, Headphones, Heart, Info, ListPlus, Pause } from "lucide-react";
import { Badge, Button, Card } from "../../shared/components/ui";
import { CompactDifficultySummary } from "../../shared/components/CompactDifficultySummary";
import { DifficultyIcon } from "../../shared/components/DifficultyIcon";
import { APP_TIME_ZONE, fullNumber } from "../../shared/lib/format";
import { cn } from "../../shared/lib/cn";
import type { OnlineBeatmapset } from "../../shared/types/osu";
import { durationLabel, normalizePreviewUrl } from "./filters";
import "../../shared/styles/beatmapCards.css";

function dateLabel(value?: string | null) {
  if (!value) return "未定";
  return new Intl.DateTimeFormat("zh-CN", { year: "numeric", month: "2-digit", day: "2-digit", timeZone: APP_TIME_ZONE }).format(new Date(value));
}

function statusTone(status: string): "success" | "pink" | "cyan" | "warning" {
  if (status === "ranked" || status === "approved") return "success";
  if (status === "loved") return "pink";
  if (status === "qualified") return "cyan";
  return "warning";
}

function statusLabel(status: string) {
  const labels: Record<string, string> = {
    ranked: "上架",
    approved: "批准",
    loved: "社区喜爱",
    qualified: "过审",
    pending: "待定",
    wip: "制作中",
    graveyard: "坟场",
  };
  return labels[status] ?? status;
}

export function BeatmapsetCard({ beatmapset, downloading, playing, selected, onAddToCollection = () => undefined, onDownload, onOpen, onPreview, onSelect }: { beatmapset: OnlineBeatmapset; downloading: boolean; playing: boolean; selected: boolean; onAddToCollection?: () => void; onDownload: () => void; onOpen: () => void; onPreview: () => void; onSelect: () => void }) {
  const preview = normalizePreviewUrl(beatmapset.preview_url);
  // 卡片内外复用同一份升序结果，避免接口顺序导致难度展示跳跃。
  const beatmaps = [...(beatmapset.beatmaps ?? [])].sort((left, right) =>
    left.difficulty_rating - right.difficulty_rating || left.id - right.id,
  );
  const longest = Math.max(0, ...beatmaps.map((beatmap) => beatmap.total_length ?? 0));
  const minStars = beatmaps.length ? Math.min(...beatmaps.map((beatmap) => beatmap.difficulty_rating)) : 0;
  const maxStars = beatmaps.length ? Math.max(...beatmaps.map((beatmap) => beatmap.difficulty_rating)) : 0;
  const objects = beatmaps.reduce((sum, beatmap) => sum + (beatmap.count_circles ?? 0) + (beatmap.count_sliders ?? 0) + (beatmap.count_spinners ?? 0), 0);
  const disabled = beatmapset.availability?.download_disabled === true;
  const cover = beatmapset.covers?.["card@2x"] ?? beatmapset.covers?.card ?? beatmapset.covers?.cover;

  return <Card
    aria-label={`查看谱面 ${beatmapset.title}`}
    className={cn(
      "opp-media-card opp-beatmap-card group relative isolate aspect-[136/55] min-h-40 min-w-0 cursor-pointer overflow-hidden rounded-xl border border-[var(--line-strong)] bg-[#1a1e24] shadow-[0_8px_22px_rgba(0,0,0,0.14)] outline-none",
      "transition-[border-color,box-shadow,transform] duration-[var(--motion-base)] ease-[cubic-bezier(.2,.8,.2,1)]",
      "after:pointer-events-none after:absolute after:inset-0 after:z-[6] after:rounded-[inherit] after:border after:border-transparent after:content-[''] after:transition-colors after:duration-[var(--motion-base)]",
      "hover:-translate-y-0.5 hover:border-[var(--theme-primary-soft)] hover:shadow-[0_16px_34px_rgba(0,0,0,0.22)] hover:after:border-[color-mix(in_srgb,var(--theme-primary)_42%,transparent)]",
      "focus-visible:ring-2 focus-visible:ring-[var(--theme-primary-soft)] motion-reduce:transition-none",
      selected && "border-[var(--theme-primary-soft)] shadow-[0_0_0_1px_var(--theme-primary-muted),0_12px_28px_rgba(0,0,0,.18)]",
    )}
    onClick={onOpen}
    onKeyDown={(event) => { if (event.key === "Enter" && event.target === event.currentTarget) onOpen(); }}
    role="button"
    tabIndex={0}
    unstyled
  >
    {cover ? <img alt="" className="opp-media-card__cover absolute inset-0 h-full w-full scale-[1.015] object-cover opacity-[.58] saturate-[.9] contrast-[1.06] [transition:transform_420ms_cubic-bezier(.2,.8,.2,1),opacity_var(--motion-base)_ease] group-hover:scale-[1.045] group-hover:opacity-[.72] motion-reduce:transition-none" src={cover} /> : null}
    <div className="opp-beatmap-card__cover-overlay absolute inset-0 z-[2] bg-[linear-gradient(90deg,rgba(17,20,25,.96),rgba(22,26,32,.82)_62%,rgba(22,26,32,.67)),linear-gradient(0deg,rgba(10,12,16,.72),transparent_70%)]" />

    <div className="opp-beatmap-card__body relative z-10 flex h-full flex-col p-4">
      <div className="opp-beatmap-card__header flex items-start gap-3">
        <button aria-label={selected ? "从下载队列移除" : "加入下载队列"} className={`opp-beatmap-card__queue grid size-9 shrink-0 place-items-center rounded-lg border backdrop-blur-md ${selected ? "border-[var(--theme-primary)] bg-[var(--theme-primary)] text-[var(--on-primary)]" : "border-white/20 bg-black/35 text-white hover:border-white/40 hover:bg-black/55"}`} disabled={disabled} onClick={(event) => { event.stopPropagation(); onSelect(); }} type="button">
          {selected ? <Check className="size-4" /> : <ListPlus className="size-4" />}
        </button>
        <div className="min-w-0 flex-1">
          <button className="block max-w-full text-left outline-none" onClick={(event) => { event.stopPropagation(); onOpen(); }} type="button">
            <h3 className="opp-beatmap-card__title truncate text-[16px] font-semibold leading-5 tracking-[-0.01em] text-white">{beatmapset.title}</h3>
            <p className="mt-0.5 truncate text-[13px] font-medium text-slate-300">{beatmapset.artist}</p>
          </button>
          <p className="mt-1.5 truncate text-xs text-slate-400">谱师 · <strong className="font-semibold text-slate-200">{beatmapset.creator}</strong></p>
        </div>
        <div className="opp-beatmap-card__actions relative z-30 flex shrink-0 gap-0.5">
          <Button aria-label="加入收藏夹" onClick={(event) => { event.stopPropagation(); onAddToCollection(); }} size="icon" title="加入收藏夹" variant="ghost"><Heart className="size-4" /></Button>
          {/* 试听属于核心操作，即使双列卡片变窄也必须常驻显示。 */}
          <Button aria-label={playing ? "暂停试听" : "试听"} disabled={!preview} onClick={(event) => { event.stopPropagation(); onPreview(); }} size="icon" title={playing ? "暂停试听" : "试听预览"} variant={playing ? "primary" : "ghost"}>{playing ? <Pause className="size-4" /> : <Headphones className="size-4" />}</Button>
          <Button aria-label="下载谱面" disabled={disabled} loading={downloading} onClick={(event) => { event.stopPropagation(); onDownload(); }} size="icon" title="下载谱面" variant="ghost">{downloading ? null : <Download className="size-4" />}</Button>
          <Button aria-label="预览详情" className="opp-beatmap-card__wide-action" onClick={(event) => { event.stopPropagation(); onOpen(); }} size="icon" variant="ghost"><Info className="size-4" /></Button>
        </div>
      </div>

      <div className="mt-auto">
        <div className="opp-beatmap-card__badges mb-3 flex flex-wrap items-center gap-2">
          <Badge tone={statusTone(beatmapset.status)}>{statusLabel(beatmapset.status)}</Badge>
          {selected ? <Badge tone="cyan">已加入队列</Badge> : null}
          {beatmapset.nsfw ? <Badge tone="warning">NSFW</Badge> : null}
          {beatmapset.video ? <Badge>VIDEO</Badge> : null}
          {disabled ? <Badge tone="warning">禁止下载</Badge> : null}
        </div>
        <CompactDifficultySummary
          items={beatmaps.map((beatmap) => ({
            id: beatmap.id,
            mode: beatmap.mode,
            stars: beatmap.difficulty_rating,
          }))}
        />
      </div>
    </div>

    {/* 悬停层随卡片比例定位，窗口缩放时保持一致的信息占比。 */}
    <div aria-hidden="true" className="opp-beatmap-card__details pointer-events-none absolute inset-x-0 bottom-0 top-[40%] z-20 translate-y-2.5 overflow-y-auto overscroll-contain border-t border-white/[.08] bg-[linear-gradient(180deg,rgba(21,25,31,.97),rgba(13,16,21,.99))] p-[14px_16px] opacity-0 [transition:opacity_var(--motion-base)_ease,transform_var(--motion-base)_cubic-bezier(.2,.8,.2,1)] group-hover:pointer-events-auto group-hover:translate-y-0 group-hover:opacity-100 group-focus-within:pointer-events-auto group-focus-within:translate-y-0 group-focus-within:opacity-100 motion-reduce:transition-none">
      <div className="opp-beatmap-card__metrics grid grid-cols-6 gap-x-2">
        {[{ label: "星级", value: `${minStars.toFixed(2)}–${maxStars.toFixed(2)}★` }, { label: "BPM", value: String(Math.round(beatmapset.bpm ?? 0)) }, { label: "长度", value: durationLabel(longest) }, { label: "物件", value: fullNumber(objects) }, { label: "游玩", value: fullNumber(beatmapset.play_count ?? 0) }, { label: "上架", value: dateLabel(beatmapset.ranked_date) }].map((metric) => <div className="min-w-0" key={metric.label}><p className="text-[10px] font-semibold uppercase tracking-[0.08em] text-slate-500">{metric.label}</p><p className="mt-0.5 truncate text-xs font-semibold text-slate-100">{metric.value}</p></div>)}
      </div>
      <div className="opp-beatmap-card__detail-difficulties mt-3 flex flex-wrap content-start gap-1.5">
        {beatmaps.map((beatmap) => <span className="inline-flex max-w-full items-center gap-1 rounded-md border border-white/10 bg-black/20 pr-2" key={beatmap.id}><DifficultyIcon className="border-0 bg-transparent px-1.5 py-1" mode={beatmap.mode} stars={beatmap.difficulty_rating} /><span className="break-all text-[11px] text-slate-200">{beatmap.version}</span></span>)}
      </div>
    </div>
  </Card>;
}
