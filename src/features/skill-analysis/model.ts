import type { SkillDimension, SkillVector } from "../../shared/types/osu";

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
