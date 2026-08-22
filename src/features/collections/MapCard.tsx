import { Hash, Trash2 } from "lucide-react";

import { Badge, Button, Card } from "../../shared/components/ui";
import type { CollectionEntry, Ruleset } from "../../shared/types/osu";

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
    <Card className="opp-map-card group overflow-hidden">
      {cover ? (
        <img
          alt=""
          className="opp-map-card__cover"
          onError={(event) => { event.currentTarget.hidden = true; }}
          src={cover}
        />
      ) : null}
      <div className="opp-map-card__shade" />

      <div className="relative z-10 flex h-full min-w-0 flex-col p-3.5">
        <div className="flex items-start justify-between gap-3">
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
          <div className="mt-3 flex min-w-0 items-center gap-2 border-t border-white/10 pt-2.5">
            <span className="opp-map-card__difficulty-dot" />
            <strong className="min-w-0 flex-1 truncate text-[13px] font-semibold text-white" title={difficulty}>{difficulty}</strong>
            <span className="shrink-0 text-[11px] text-slate-400">谱师 · {entry.creator || "未知"}</span>
          </div>
        </div>
      </div>

      {/* 悬停时只补充具体难度的技术标识，不改变卡片尺寸。 */}
      <div aria-hidden="true" className="opp-map-card__hover">
        <div className="grid grid-cols-2 gap-x-4 gap-y-3">
          <div>
            <p className="opp-map-card__metric-label">难度 ID</p>
            <p className="opp-map-card__metric-value">{entry.beatmap_id ? `#${entry.beatmap_id}` : "未解析"}</p>
          </div>
          <div>
            <p className="opp-map-card__metric-label">谱面集 ID</p>
            <p className="opp-map-card__metric-value">{entry.beatmapset_id ? `#${entry.beatmapset_id}` : "未解析"}</p>
          </div>
          <div className="col-span-2">
            <p className="opp-map-card__metric-label">校验信息</p>
            <p className="opp-map-card__metric-value flex items-center gap-1.5"><Hash className="size-3" />{entry.checksum ? "已记录精确 MD5" : "等待本地谱面解析"}</p>
          </div>
        </div>
      </div>
    </Card>
  );
}
