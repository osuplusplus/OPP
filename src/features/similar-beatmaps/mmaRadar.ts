import type { ManiaPatternView } from "../../shared/types/osu";

export const mmaAxes = ["Stream", "Chordstream", "Jacks", "Coordination", "Density", "Wildcard"];

export interface MmaAxisValue {
  dimension: string;
  radius: number;
  coverage: number | null;
  seconds: number | null;
  specifics: string;
}

/**
 * 六轴半径固定为覆盖率的平方根：六类覆盖率是相互重叠的真实时长占比，
 * 不按最大值或总和归一化，因此参考谱面与候选谱面始终共用同一把尺子。
 */
export function mmaRadarValues(view: ManiaPatternView | null | undefined): MmaAxisValue[] {
  return mmaAxes.map((dimension, index) => {
    const value = view?.coverage?.[index];
    const coverage = typeof value === "number" && Number.isFinite(value) ? Math.max(0, Math.min(1, value)) : null;
    const duration = view?.duration_seconds;
    return {
      dimension,
      radius: coverage === null ? 0 : Math.sqrt(coverage),
      coverage,
      seconds: coverage === null || typeof duration !== "number" || !Number.isFinite(duration) ? null : coverage * duration,
      specifics: view?.bars.find((bar) => bar.pattern === dimension)?.specific_types.slice(0, 3).map(([name]) => name).join(" · ") ?? "",
    };
  });
}
