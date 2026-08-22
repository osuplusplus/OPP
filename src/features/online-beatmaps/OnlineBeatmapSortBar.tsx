import { ArrowDown, ArrowUp, ListFilter } from "lucide-react";
import { cn } from "../../shared/lib/cn";

const sortFields = [
  { key: "relevance", label: "相关度", defaultDirection: "desc", fixed: true },
  { key: "title", label: "标题", defaultDirection: "asc", fixed: false },
  { key: "artist", label: "艺术家", defaultDirection: "asc", fixed: false },
  { key: "difficulty", label: "难度", defaultDirection: "desc", fixed: false },
  { key: "ranked", label: "上架时间", defaultDirection: "desc", fixed: false },
  { key: "rating", label: "评分", defaultDirection: "desc", fixed: false },
  { key: "plays", label: "游玩次数", defaultDirection: "desc", fixed: false },
  { key: "favourites", label: "收藏量", defaultDirection: "desc", fixed: false },
] as const;

type SortField = (typeof sortFields)[number];
type SortDirection = "asc" | "desc";

function sortValue(field: SortField, direction: SortDirection) {
  return `${field.key}_${field.fixed ? "desc" : direction}`;
}

export function OnlineBeatmapSortBar({
  sort,
  onChange,
}: {
  sort: string;
  onChange: (sort: string) => void;
}) {
  return (
    <div aria-label="谱面排序方式" className="opp-online-panel mb-4 flex min-h-12 items-center gap-2.5 rounded-[11px] border border-[var(--line-subtle)] bg-[color-mix(in_srgb,var(--surface-panel)_94%,transparent)] p-[7px_10px] shadow-[0_14px_34px_rgba(0,0,0,0.08)]" role="toolbar">
      <span className="inline-flex shrink-0 items-center gap-1.5 border-r border-[var(--line-subtle)] p-[4px_12px_4px_3px] text-[11px] font-[650] tracking-[.04em] text-[var(--text-muted)]"><ListFilter className="size-3.5" />排序</span>
      <div className="flex min-w-0 flex-1 flex-wrap items-center gap-1">
        {sortFields.map((field) => {
          const ascending = sort === `${field.key}_asc`;
          const descending = sort === `${field.key}_desc`;
          const active = ascending || descending;
          const nextDirection: SortDirection = active
            ? ascending ? "desc" : "asc"
            : field.defaultDirection;
          const directionLabel = ascending ? "升序" : "降序";

          return (
            <button
              aria-label={active && !field.fixed ? `${field.label}，当前${directionLabel}，点击切换` : `按${field.label}排序`}
              aria-pressed={active}
              className={cn(
                "inline-flex min-h-[30px] items-center gap-1 rounded-[7px] border-0 bg-transparent p-[5px_9px] text-xs font-[560] text-[var(--text-muted)] transition-[color,background-color,box-shadow] duration-[var(--motion-fast)] hover:bg-[var(--surface-interactive-hover)] hover:text-[var(--text)]",
                active && "bg-[var(--theme-primary-muted)] [background-image:none] text-[var(--theme-primary-light)] shadow-[inset_0_-2px_0_var(--theme-primary)]",
              )}
              key={field.key}
              onClick={() => onChange(sortValue(field, nextDirection))}
              type="button"
            >
              {field.label}
              {active && !field.fixed ? (ascending ? <ArrowUp className="size-3" /> : <ArrowDown className="size-3" />) : null}
            </button>
          );
        })}
      </div>
    </div>
  );
}
