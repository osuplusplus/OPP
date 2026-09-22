import type { CollectionBrowseRow, CollectionPersonalRecord } from "../../shared/types/osu";

export function collectionDuration(milliseconds: number | null | undefined) {
  if (milliseconds == null || !Number.isFinite(milliseconds)) return "—";
  const seconds = Math.max(0, Math.floor(milliseconds / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}

export function collectionMetric(value: number | null | undefined, digits = 1) {
  return value == null || !Number.isFinite(value) ? "—" : Number(value.toFixed(digits)).toString();
}

export function collectionSlotGroup(slot: string) {
  return slot.match(/^[^\d]+/)?.[0].trim().toUpperCase() || "未分配";
}

export function collectionScoreSource(score: CollectionPersonalRecord["representative"]) {
  if (!score) return "";
  const scoring: Record<string, string> = { stable: "Stable", score_v2: "Score V2", lazer: "lazer", lazer_legacy: "lazer · Legacy", manual: "手动" };
  return scoring[score.scoring] ?? score.scoring;
}

export function localCollectionRoute(row: CollectionBrowseRow) {
  const local = row.local;
  return local ? `/local/maps?${new URLSearchParams({ client: local.resource.client, ruleset: local.ruleset, resource: local.resource.resource_id, set: local.set_key })}` : null;
}
export function scoreLabel(score: CollectionPersonalRecord["representative"]) {
  return score ? `${score.score.toLocaleString()}${score.accuracy == null ? "" : ` · ${(score.accuracy * 100).toFixed(2)}%`}` : "暂无成绩";
}
export function highlightParts(text: string, query: string): { text: string; matched: boolean }[] {
  const tokens = query.trim().split(/\s+/).filter(Boolean).map((s) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
  if (!tokens.length) return [{ text, matched: false }];
  const regex = new RegExp(`(${tokens.join("|")})`, "giu");
  return text.split(regex).filter(Boolean).map((part) => ({ text: part, matched: new RegExp(`^(?:${tokens.join("|")})$`, "iu").test(part) }));
}
export function readSession<T>(key: string, fallback: T): T {
  try { return JSON.parse(sessionStorage.getItem(key) || "null") as T ?? fallback; } catch { return fallback; }
}
export function saveSession(key: string, value: unknown) {
  try { sessionStorage.setItem(key, JSON.stringify(value)); } catch { /* Browsing works without session storage. */ }
}
