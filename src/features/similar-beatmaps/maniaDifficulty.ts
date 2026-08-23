import type { ManiaDifficultyVector } from "../../shared/types/osu";

const keys: Array<keyof ManiaDifficultyVector> = [
  "speed",
  "hand_stream",
  "jack",
  "chordjack",
  "technical",
  "stamina",
  "long_note",
  "course",
];

export function maniaSkillProfile(vector: ManiaDifficultyVector): ManiaDifficultyVector {
  // Analyzer difficulty dimensions are already normalized to a comparable
  // absolute scale.  Normalizing by the sum made every radar chart have a
  // maximum of roughly 1/8, which was especially noticeable for difficult
  // maps. Keep the absolute intensity so hard maps occupy more of the chart.
  const values = keys.map((key) => {
    const value = Number(vector[key]);
    return Number.isFinite(value) ? Math.max(0, value) : 0;
  });
  const profile = {} as ManiaDifficultyVector;
  keys.forEach((key, index) => {
    profile[key] = Math.min(1, values[index]);
  });
  return profile;
}
