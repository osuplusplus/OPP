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

/**
 * 六轴雷达只在两侧都有键型数据时绘制对比多边形；缺一侧时只画已有的一侧，
 * 避免把没有数据的一侧画成覆盖率为 0 的形状。旧版难度雷达沿用原判断。
 */
export function showsRadarComparison(
  patternView: ManiaPatternView | null | undefined,
  patternViewComparison: ManiaPatternView | null | undefined,
  hasDifficultyComparison: boolean,
): boolean {
  return patternView ? Boolean(patternViewComparison) : hasDifficultyComparison;
}
