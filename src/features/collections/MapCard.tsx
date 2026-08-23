import { Hash, Trash2 } from "lucide-react";

import { Badge, Button, Card } from "../../shared/components/ui";
import type { CollectionEntry, Ruleset } from "../../shared/types/osu";
import "../../shared/styles/beatmapCards.css";

const modeLabels: Record<Ruleset, string> = {
  osu: "osu!",
  taiko: "taiko",
  fruits: "catch",
  mania: "mania",
};

function coverUrl(beatmapsetId: number | null) {
  return beatmapsetId ? `https://assets.ppy.sh/beatmaps/${beatmapsetId}/covers/card@2x.jpg` : null;
}

export function MapCard({
  entry,
  busy,
  readOnly,
  onRemove,
}: {
  entry: CollectionEntry;
  busy: boolean;
  readOnly: boolean;
  onRemove: () => void;
}) {
  const cover = coverUrl(entry.beatmapset_id);
  const title = entry.title || `谱面 #${entry.beatmap_id ?? "未知"}`;
  const difficulty = entry.difficulty_name || "未解析难度";
  const mode = entry.ruleset ? modeLabels[entry.ruleset] : "未知模式";

  return (
    <Card
      className="opp-media-card opp-map-card group relative isolate aspect-[31/16] min-h-40 min-w-0 overflow-hidden rounded-[10px] border border-[var(--line-strong)] bg-[#1a1e24] shadow-[0_7px_18px_rgba(0,0,0,.13)] [transition:border-color_var(--motion-base)_ease,box-shadow_var(--motion-base)_ease,transform_var(--motion-base)_cubic-bezier(.2,.8,.2,1)] hover:-translate-y-0.5 hover:border-[var(--theme-primary-soft)] hover:shadow-[0_13px_28px_rgba(0,0,0,.21)] motion-reduce:transition-none"
      unstyled
    >
      {cover ? (
        <img
          alt=""
          className="absolute inset-0 h-full w-full scale-[1.02] object-cover opacity-[.52] saturate-[.86] contrast-[1.06] [transition:opacity_var(--motion-base)_ease,transform_420ms_cubic-bezier(.2,.8,.2,1)] group-hover:scale-[1.055] group-hover:opacity-[.7] motion-reduce:transition-none"
          onError={(event) => { event.currentTarget.hidden = true; }}
          src={cover}
        />
      ) : null}
      <div className="absolute inset-0 z-[2] h-full w-full bg-[linear-gradient(90deg,rgba(16,19,24,.97),rgba(20,24,30,.77)_68%,rgba(20,24,30,.62))]" />

      <div className="opp-map-card__body relative z-10 flex h-full min-w-0 flex-col p-3">
        <div className="opp-map-card__header flex items-start justify-between gap-3">
          <div className="flex min-w-0 flex-wrap gap-1.5">
            <Badge tone="cyan">{mode}</Badge>
            <Badge tone={entry.resolved ? "success" : "warning"}>
              {entry.resolved ? "已在本地" : "待补齐"}
            </Badge>
          </div>
          {!readOnly ? (
            <Button
              aria-label={`从收藏夹移除 ${title} ${difficulty}`}
              disabled={busy}
              onClick={onRemove}
              size="icon"
              title="从收藏夹移除"
              variant="ghost"
            >
              <Trash2 className="size-3.5" />
            </Button>
          ) : null}
        </div>

        <div className="mt-auto min-w-0">
          <h3 className="truncate text-[15px] font-semibold tracking-[-0.01em] text-white" title={title}>{title}</h3>
          <p className="mt-0.5 truncate text-xs font-medium text-slate-300">{entry.artist || "未知艺术家"}</p>
          <div className="opp-map-card__difficulty-row mt-2 flex min-w-0 items-center gap-2 border-t border-white/10 pt-2">
            <span className="h-[1.05rem] w-[3px] shrink-0 rounded-full bg-[linear-gradient(180deg,var(--theme-primary-light),var(--theme-secondary))] shadow-[0_0_12px_var(--theme-primary-glow)]" />
            <strong className="min-w-0 flex-1 truncate text-[13px] font-semibold text-white" title={difficulty}>{difficulty}</strong>
            <span className="opp-map-card__creator shrink-0 text-[11px] text-slate-400">谱师 · {entry.creator || "未知"}</span>
          </div>
        </div>
      </div>

      {/* 悬停层按比例定位，始终为具体难度信息保留稳定空间。 */}
      <div aria-hidden="true" className="opp-map-card__details pointer-events-none absolute inset-x-0 bottom-0 top-[28.5%] z-20 translate-y-2 border-t border-white/[.08] bg-[linear-gradient(180deg,rgba(21,25,31,.98),rgba(13,16,21,.99))] p-[15px] opacity-0 [transition:opacity_var(--motion-base)_ease,transform_var(--motion-base)_cubic-bezier(.2,.8,.2,1)] group-hover:translate-y-0 group-hover:opacity-100 motion-reduce:transition-none">
        <div className="opp-map-card__metrics grid grid-cols-3 gap-x-3">
          <div>
            <p className="text-[10px] font-[650] uppercase tracking-[.08em] text-[#7f8b9b]">难度 ID</p>
            <p className="mt-0.5 overflow-hidden text-ellipsis whitespace-nowrap text-xs font-semibold text-[#eef2f7]">{entry.beatmap_id ? `#${entry.beatmap_id}` : "未解析"}</p>
          </div>
          <div>
            <p className="text-[10px] font-[650] uppercase tracking-[.08em] text-[#7f8b9b]">谱面集 ID</p>
            <p className="mt-0.5 overflow-hidden text-ellipsis whitespace-nowrap text-xs font-semibold text-[#eef2f7]">{entry.beatmapset_id ? `#${entry.beatmapset_id}` : "未解析"}</p>
          </div>
          <div className="opp-map-card__checksum min-w-0">
            <p className="text-[10px] font-[650] uppercase tracking-[.08em] text-[#7f8b9b]">校验信息</p>
            <p className="mt-0.5 flex items-center gap-1.5 overflow-hidden text-ellipsis whitespace-nowrap text-xs font-semibold text-[#eef2f7]"><Hash className="size-3" />{entry.checksum ? "已记录精确 MD5" : "等待本地谱面解析"}</p>
          </div>
        </div>
      </div>
    </Card>
  );
}
