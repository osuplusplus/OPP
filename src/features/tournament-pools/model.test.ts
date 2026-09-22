import { expect, it } from "vitest";
import type { TournamentPool, TournamentPoolEntry } from "../../shared/types/osu";
import { poolDownloads } from "./model";

const entry = (id: number, setId: number | null, disabled = false): TournamentPoolEntry => ({
  beatmap_id: id, selection_type: "NM", position: id, selected_by: null, selected_by_name: null,
  comment: "", is_custom: false, is_original: false, resolution_error: null,
  beatmap: setId === null ? null : { beatmapset_id: setId, title: "Song", artist: "Artist", creator: "Mapper", difficulty_name: "Hard", checksum: null, download_disabled: disabled },
});

it("groups sets while preserving every expected pool difficulty and reporting unavailable entries", () => {
  const pool: TournamentPool = { reference: { provider: "rino", season: "s1", category: "qualification" }, title: "Pool",
    entries: [entry(1, 10), entry(2, 10), entry(1, 10), entry(3, 20), entry(4, null), entry(5, 30, true)] };
  const result = poolDownloads(pool);
  expect(result.items.map((set) => ({ id: set.id, beatmaps: set.beatmaps }))).toEqual([
    { id: 10, beatmaps: [{ id: 1 }, { id: 2 }] }, { id: 20, beatmaps: [{ id: 3 }] },
  ]);
  expect(result.unavailable).toEqual([4, 5]);
  expect(result.items.every((set) => set.allow_extra_difficulties)).toBe(true);
  expect(poolDownloads(undefined)).toEqual({ items: [], unavailable: [] });
});
