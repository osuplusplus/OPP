import type { OsekaiMedal } from "../../shared/types/osu";
import translations from "./medalTranslations.json";

export type MedalTranslation = Pick<OsekaiMedal, "Name" | "Description" | "Instructions" | "Solution">;

// Generated translations can be added here by Medal_ID. Keeping this as a
// separate resource makes periodic Osekai updates safe: untranslated medals
// continue to use the live API response.
export const medalTranslations: Record<string, MedalTranslation> = translations;

export function localizeMedal<T extends OsekaiMedal>(medal: T): T {
  const translation = medalTranslations[String(medal.Medal_ID)];
  return translation ? { ...medal, ...translation } : medal;
}
