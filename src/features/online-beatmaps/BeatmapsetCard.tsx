import { Check, Download, Headphones, Heart, Info, ListPlus, Pause } from "lucide-react";
import { Badge, Button, Card } from "../../shared/components/ui";
import { DifficultyIcon } from "../../shared/components/DifficultyIcon";
import { APP_TIME_ZONE, fullNumber } from "../../shared/lib/format";
import type { OnlineBeatmapset } from "../../shared/types/osu";
import { durationLabel, normalizePreviewUrl } from "./filters";

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
  const beatmaps = beatmapset.beatmaps ?? [];
  const longest = Math.max(0, ...beatmaps.map((beatmap) => beatmap.total_length ?? 0));
  const minStars = beatmaps.length ? Math.min(...beatmaps.map((beatmap) => beatmap.difficulty_rating)) : 0;
  const maxStars = beatmaps.length ? Math.max(...beatmaps.map((beatmap) => beatmap.difficulty_rating)) : 0;
  const objects = beatmaps.reduce((sum, beatmap) => sum + (beatmap.count_circles ?? 0) + (beatmap.count_sliders ?? 0) + (beatmap.count_spinners ?? 0), 0);
  const disabled = beatmapset.availability?.download_disabled === true;
  const cover = beatmapset.covers?.["card@2x"] ?? beatmapset.covers?.card ?? beatmapset.covers?.cover;

  return <Card
    aria-label={`查看谱面 ${beatmapset.title}`}
    className={`opp-beatmap-card group cursor-pointer overflow-hidden ${selected ? "is-selected" : ""}`}
    onClick={onOpen}
    onKeyDown={(event) => { if (event.key === "Enter" && event.target === event.currentTarget) onOpen(); }}
    role="button"
    tabIndex={0}
  >
    {cover ? <img alt="" className="opp-beatmap-card__cover" src={cover} /> : null}
    <div className="opp-beatmap-card__shade" />

    <div className="relative z-10 flex h-full flex-col p-4">
      <div className="flex items-start gap-3">
        <button aria-label={selected ? "从下载队列移除" : "加入下载队列"} className={`grid size-9 shrink-0 place-items-center rounded-lg border backdrop-blur-md ${selected ? "border-[var(--theme-primary)] bg-[var(--theme-primary)] text-[var(--on-primary)]" : "border-white/20 bg-black/35 text-white hover:border-white/40 hover:bg-black/55"}`} disabled={disabled} onClick={(event) => { event.stopPropagation(); onSelect(); }} type="button">
          {selected ? <Check className="size-4" /> : <ListPlus className="size-4" />}
        </button>
        <div className="min-w-0 flex-1">
          <button className="block max-w-full text-left outline-none" onClick={(event) => { event.stopPropagation(); onOpen(); }} type="button">
            <h3 className="truncate text-[16px] font-semibold leading-5 tracking-[-0.01em] text-white">{beatmapset.title}</h3>
            <p className="mt-0.5 truncate text-[13px] font-medium text-slate-300">{beatmapset.artist}</p>
          </button>
          <p className="mt-1.5 truncate text-xs text-slate-400">谱师 · <strong className="font-semibold text-slate-200">{beatmapset.creator}</strong></p>
        </div>
        <div className="opp-beatmap-card__actions relative z-30 flex shrink-0 gap-0.5">
          <Button aria-label="加入收藏夹" onClick={(event) => { event.stopPropagation(); onAddToCollection(); }} size="icon" title="加入收藏夹" variant="ghost"><Heart className="size-4" /></Button>
          <Button aria-label={playing ? "暂停试听" : "试听"} disabled={!preview} onClick={(event) => { event.stopPropagation(); onPreview(); }} size="icon" variant={playing ? "primary" : "ghost"}>{playing ? <Pause className="size-4" /> : <Headphones className="size-4" />}</Button>
          <Button aria-label="下载谱面" disabled={disabled} loading={downloading} onClick={(event) => { event.stopPropagation(); onDownload(); }} size="icon" title="下载谱面" variant="ghost">{downloading ? null : <Download className="size-4" />}</Button>
          <Button aria-label="预览详情" onClick={(event) => { event.stopPropagation(); onOpen(); }} size="icon" variant="ghost"><Info className="size-4" /></Button>
        </div>
      </div>

      <div className="mt-auto">
        <div className="mb-3 flex flex-wrap items-center gap-2">
          <Badge tone={statusTone(beatmapset.status)}>{statusLabel(beatmapset.status)}</Badge>
          {selected ? <Badge tone="cyan">已加入队列</Badge> : null}
          {beatmapset.nsfw ? <Badge tone="warning">NSFW</Badge> : null}
          {beatmapset.video ? <Badge>VIDEO</Badge> : null}
          {disabled ? <Badge tone="warning">禁止下载</Badge> : null}
        </div>
        <div className="opp-beatmap-card__difficulty-summary">
          <span className="mr-1 shrink-0 text-[11px] font-semibold text-slate-400">难度</span>
          {beatmaps.slice(0, 5).map((beatmap) => <span className="inline-flex min-w-0 items-center gap-1.5" key={beatmap.id}><DifficultyIcon className="px-1.5 py-0.5" mode={beatmap.mode} stars={beatmap.difficulty_rating} /><span className="max-w-24 truncate text-[11px] text-slate-200">{beatmap.version}</span></span>)}
          {beatmaps.length > 5 ? <span className="text-[11px] font-medium text-slate-400">+{beatmaps.length - 5}</span> : null}
        </div>
      </div>
    </div>

    {/* 悬停层不改变卡片高度，避免多列列表出现布局跳动。 */}
    <div aria-hidden="true" className="opp-beatmap-card__hover">
      <div className="grid grid-cols-3 gap-x-4 gap-y-2.5">
        {[{ label: "星级", value: `${minStars.toFixed(2)}–${maxStars.toFixed(2)}★` }, { label: "BPM", value: String(Math.round(beatmapset.bpm ?? 0)) }, { label: "长度", value: durationLabel(longest) }, { label: "物件", value: fullNumber(objects) }, { label: "游玩", value: fullNumber(beatmapset.play_count ?? 0) }, { label: "上架", value: dateLabel(beatmapset.ranked_date) }].map((metric) => <div className="min-w-0" key={metric.label}><p className="text-[10px] font-semibold uppercase tracking-[0.08em] text-slate-500">{metric.label}</p><p className="mt-0.5 truncate text-xs font-semibold text-slate-100">{metric.value}</p></div>)}
      </div>
      <div className="mt-3 flex max-h-[58px] flex-wrap content-start gap-1.5 overflow-hidden">
        {beatmaps.map((beatmap) => <span className="inline-flex min-w-0 items-center gap-1 rounded-md border border-white/10 bg-black/20 pr-2" key={beatmap.id}><DifficultyIcon className="border-0 bg-transparent px-1.5 py-1" mode={beatmap.mode} stars={beatmap.difficulty_rating} /><span className="max-w-28 truncate text-[11px] text-slate-200">{beatmap.version}</span></span>)}
      </div>
    </div>
  </Card>;
}
