import type { MouseEvent } from "react";
import type { Ruleset } from "../types/osu";
import { DifficultyIcon } from "./DifficultyIcon";

export interface CompactDifficultySummaryItem {
  id: number | string;
  mode: Ruleset;
  stars: number | null | undefined;
  onClick?: () => void;
}

function DifficultyItem({ item }: { item: CompactDifficultySummaryItem }) {
  const content = (
    <DifficultyIcon
      className="opp-difficulty-summary__rating px-1.5 py-0.5"
      mode={item.mode}
      stars={item.stars}
    />
  );

  if (!item.onClick) {
    return <span className="opp-difficulty-summary__item">{content}</span>;
  }

  return (
    <button
      className="opp-difficulty-summary__item outline-none hover:brightness-110 focus-visible:ring-2 focus-visible:ring-[var(--theme-primary-soft)]"
      onClick={(event: MouseEvent<HTMLButtonElement>) => {
        event.stopPropagation();
        item.onClick?.();
      }}
      type="button"
    >
      {content}
    </button>
  );
}

function HiddenCount({ count, visibleAt }: { count: number; visibleAt: 3 | 4 | 5 }) {
  if (count <= 0) return null;

  return (
    <span
      aria-label={`还有 ${count} 个难度`}
      className={`opp-difficulty-summary__count opp-difficulty-summary__count--${visibleAt}`}
      title={`还有 ${count} 个难度`}
    >
      +{count}
    </span>
  );
}

export function CompactDifficultySummary({
  items,
}: {
  items: CompactDifficultySummaryItem[];
}) {
  const visibleItems = items.slice(0, 5);

  return (
    // 紧凑摘要按卡片实际宽度切换可见数量，计数始终独占一列，避免徽标互相挤压。
    <div className="opp-difficulty-summary">
      <span className="opp-difficulty-summary__label">难度</span>
      <div className="opp-difficulty-summary__items">
        {visibleItems.map((item) => (
          <DifficultyItem item={item} key={item.id} />
        ))}
      </div>
      <HiddenCount count={items.length - 3} visibleAt={3} />
      <HiddenCount count={items.length - 4} visibleAt={4} />
      <HiddenCount count={items.length - 5} visibleAt={5} />
    </div>
  );
}
