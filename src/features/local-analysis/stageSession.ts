import type { BeatmapQuery, OsuClient, Ruleset } from "../../shared/types/osu";
export interface StageSession { query: BeatmapQuery; setKey?: string; difficultyId?: string }
const key = (client: OsuClient, ruleset: Ruleset) => `opp:local-stage:${client}:${ruleset}`;
export function readStageSession(client: OsuClient, ruleset: Ruleset): StageSession | null {
  try { return JSON.parse(window.localStorage.getItem(key(client,ruleset)) || "null") as StageSession | null; } catch { return null; }
}
export function writeStageSession(client: OsuClient, ruleset: Ruleset, session: StageSession) {
  try { window.localStorage.setItem(key(client,ruleset),JSON.stringify(session)); } catch { /* A disabled store must not block browsing. */ }
}
