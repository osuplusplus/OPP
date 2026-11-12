import type { SkillContribution, SkillCoverage, SkillDimension, SkillVector } from "../../shared/types/osu";

export type SkillFocus = SkillDimension | "all";

export const skillDimensions: Array<{ key: SkillDimension; label: string }> = [
  { key: "stamina", label: "耐力" }, { key: "tenacity", label: "连打" },
  { key: "agility", label: "机动" }, { key: "accuracy", label: "准确" },
  { key: "precision", label: "精度" }, { key: "reaction", label: "反应" },
  { key: "memory", label: "记忆" }, { key: "reading", label: "读图" },
];

export function orderedSkills(skills: SkillVector) {
  return skillDimensions.map(({ key, label }) => ({ key, label, value: Number(skills[key] ?? 0) }))
    .sort((left, right) => right.value - left.value);
}

export function skillRadarData(skills: SkillVector) {
  return skillDimensions.map(({ key, label }) => ({ dimension: label, value: Number(skills[key] ?? 0) }));
}

export function formatSkill(value: number) {
  return Number.isFinite(value) ? value.toFixed(0) : "—";
}

/** 明细过滤 + 排序：搜索匹配标题/作者/版本；维度聚焦时仅保留有贡献的成绩并按贡献降序，否则按 PP 降序。 */
export function filterContributions(
  contributions: SkillContribution[],
  { search, focus }: { search: string; focus: SkillFocus },
): SkillContribution[] {
  const normalized = search.trim().toLowerCase();
  return contributions
    .filter((item) => {
      const matchesSearch = !normalized || `${item.title} ${item.artist} ${item.version}`.toLowerCase().includes(normalized);
      return matchesSearch && (focus === "all" || item.weighted_skills[focus] > 0);
    })
    .sort((left, right) => focus === "all"
      ? (right.pp ?? 0) - (left.pp ?? 0)
      : right.weighted_skills[focus] - left.weighted_skills[focus]);
}

/** 覆盖率百分比（0-100），无请求成绩时为 0。 */
export function coveragePercent(coverage: SkillCoverage) {
  return coverage.requested_scores
    ? Math.round((coverage.analyzed_scores / coverage.requested_scores) * 100)
    : 0;
}

/** 本次真正新计算的成绩数 = 已分析 - 增量复用。 */
export function newlyComputed(coverage: SkillCoverage) {
  return Math.max(0, coverage.analyzed_scores - (coverage.reused_scores ?? 0));
}
