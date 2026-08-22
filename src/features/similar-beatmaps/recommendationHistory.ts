import type {
  AnySimilarityResult,
  ManiaKeyCount,
  SimilarityRuleset,
} from "../../shared/types/osu";

const STORAGE_KEY = "opp.similarity-recommendation-history.v2";
const LEGACY_STORAGE_KEY = "opp.similarity-recommendation-history.v1";

export interface RecommendationHistoryEntry {
  displayed_at: string;
  ruleset: SimilarityRuleset;
  key_count: ManiaKeyCount | null;
  result: AnySimilarityResult;
}

interface StoredRecommendationHistory {
  version: 2;
  day: string;
  entries: RecommendationHistoryEntry[];
}

function localDay(date = new Date()) {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function normalizeLegacyResult(value: unknown): AnySimilarityResult | null {
  if (!value || typeof value !== "object") return null;
  const result = value as Record<string, unknown>;
  const beatmapId = Number(result.beatmap_id);
  if (!Number.isSafeInteger(beatmapId) || beatmapId <= 0) return null;
  const ruleset = result.ruleset === "mania" ? "mania" : "osu";
  const recommendedBy = result.recommended_by;
  return {
    ...result,
    ruleset,
    ...(recommendedBy && typeof recommendedBy === "object"
      ? { recommended_by: { ...(recommendedBy as Record<string, unknown>), ruleset } }
      : {}),
  } as AnySimilarityResult;
}

function normalizeEntry(value: unknown): RecommendationHistoryEntry | null {
  if (!value || typeof value !== "object") return null;
  const entry = value as Record<string, unknown>;
  const result = normalizeLegacyResult(entry.result);
  if (!result || typeof entry.displayed_at !== "string") return null;
  const ruleset = entry.ruleset === "mania" || result.ruleset === "mania" ? "mania" : "osu";
  const rawKeyCount = Number(entry.key_count ?? (result.ruleset === "mania" ? result.key_count : NaN));
  const keyCount = ruleset === "mania" && (rawKeyCount === 4 || rawKeyCount === 6 || rawKeyCount === 7)
    ? rawKeyCount
    : null;
  return { displayed_at: entry.displayed_at, ruleset, key_count: keyCount, result } as RecommendationHistoryEntry;
}

function emptyHistory(): StoredRecommendationHistory {
  return { version: 2, day: localDay(), entries: [] };
}

function parseStoredHistory(raw: string | null): StoredRecommendationHistory | null {
  try {
    const parsed = JSON.parse(raw ?? "null") as Record<string, unknown> | null;
    if (!parsed || parsed.day !== localDay() || !Array.isArray(parsed.entries)) return null;
    return {
      version: 2,
      day: parsed.day,
      entries: parsed.entries.map(normalizeEntry).filter((entry): entry is RecommendationHistoryEntry => entry !== null),
    };
  } catch {
    return null;
  }
}

function writeStoredHistory(history: StoredRecommendationHistory) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(history));
  } catch {
    // Storage failures must not prevent recommendations from being displayed.
  }
}

function readStoredHistory(): StoredRecommendationHistory {
  const current = parseStoredHistory(localStorage.getItem(STORAGE_KEY));
  if (current) return current;
  const migrated = parseStoredHistory(localStorage.getItem(LEGACY_STORAGE_KEY));
  if (migrated) {
    writeStoredHistory(migrated);
    return migrated;
  }
  return emptyHistory();
}

export function getTodayRecommendationHistory(ruleset: SimilarityRuleset = "osu") {
  return readStoredHistory().entries.filter((entry) => entry.ruleset === ruleset);
}

export function getTodayRecommendedBeatmapIds(ruleset: SimilarityRuleset = "osu") {
  return new Set(getTodayRecommendationHistory(ruleset).map((entry) => entry.result.beatmap_id));
}

export function excludeTodayRecommendedResults<T extends AnySimilarityResult>(
  results: readonly T[],
  ruleset: SimilarityRuleset,
  additionalExcludedIds: Iterable<number> = [],
) {
  const excludedIds = getTodayRecommendedBeatmapIds(ruleset);
  for (const beatmapId of additionalExcludedIds) excludedIds.add(beatmapId);
  return results.filter((result) => !excludedIds.has(result.beatmap_id));
}

export function recordDisplayedRecommendationBatch(
  results: AnySimilarityResult[],
  ruleset: SimilarityRuleset = "osu",
  expectedBatchSize = 5,
) {
  if (!results.length || results.length !== expectedBatchSize) return getTodayRecommendationHistory(ruleset);

  const history = readStoredHistory();
  const knownIds = new Set(
    history.entries
      .filter((entry) => entry.ruleset === ruleset)
      .map((entry) => entry.result.beatmap_id),
  );
  const displayedAt = new Date().toISOString();
  for (const result of results) {
    if (result.ruleset !== ruleset || knownIds.has(result.beatmap_id)) continue;
    history.entries.push({
      displayed_at: displayedAt,
      ruleset,
      key_count: result.ruleset === "mania" ? result.key_count : null,
      result,
    });
    knownIds.add(result.beatmap_id);
  }
  writeStoredHistory(history);
  return history.entries.filter((entry) => entry.ruleset === ruleset);
}
