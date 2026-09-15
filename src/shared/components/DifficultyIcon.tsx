import type { Ruleset } from "../types/osu";
import { cn } from "../lib/cn";
import type { CSSProperties } from "react";

const iconModes: Record<Ruleset, "std" | "taiko" | "ctb" | "mania"> = {
  osu: "std",
  taiko: "taiko",
  fruits: "ctb",
  mania: "mania",
};

/**
 * Uses the static, star-specific artwork published by osu-difficulty-icons.
 * The collection is capped at 9.0★, so higher ratings use its highest icon.
 */
// eslint-disable-next-line react-refresh/only-export-components
export function difficultyIconUrl(mode: Ruleset, stars: number) {
  const starRating = Math.min(9, Math.max(0, Math.round(stars * 10) / 10)).toFixed(1);
  return `https://raw.githubusercontent.com/hiderikzki/osu-difficulty-icons/main/rendered/${iconModes[mode]}/stars_${starRating}@2x.png`;
}

export function DifficultyIcon({
  stars,
  mode = "osu",
  showValue = true,
  className,
  style,
}: {
  stars: number | null | undefined;
  mode?: Ruleset;
  showValue?: boolean;
  className?: string;
  style?: CSSProperties & Record<`--${string}`, string | number>;
}) {
  const validStars = typeof stars === "number" && Number.isFinite(stars);

  return (
    <span
      className={cn(
        "inline-flex shrink-0 items-center gap-1.5 rounded-lg border border-white/[0.12] bg-black/20 px-2 py-1 font-mono text-xs font-semibold text-slate-100",
        className,
      )}
      title={validStars ? `${stars.toFixed(2)} stars` : "Star rating unavailable"}
      style={style}
    >
      {validStars ? (
        <img
          alt=""
          className="size-4 object-contain"
          height={32}
          src={difficultyIconUrl(mode, stars)}
          width={32}
        />
      ) : null}
      {showValue || !validStars ? (validStars ? stars.toFixed(2) : "-") : null}
    </span>
  );
}
