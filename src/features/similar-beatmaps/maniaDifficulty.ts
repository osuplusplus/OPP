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
  const values = keys.map((key) => vector[key]);
  const mean = values.reduce((sum, value) => sum + value, 0) / values.length;
  const shaped = values.map((value) => Math.exp((value - mean) * 4));
  const total = shaped.reduce((sum, value) => sum + value, 0) || 1;
  const profile = {} as ManiaDifficultyVector;
  keys.forEach((key, index) => {
    profile[key] = shaped[index] / total;
  });
  return profile;
}
