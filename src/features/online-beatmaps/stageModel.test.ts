import { expect, it } from "vitest";
import { createDefaultSearchQuery } from "./filters";
import { onlineResultsTitle, searchSameTitle, trendingQuery } from "./stageModel";

it("keeps trends restricted to the current mode and the last seven days in application time", () => {
  expect(trendingQuery("mania", new Date("2026-09-15T20:00:00Z"))).toMatchObject({
    ruleset: "mania", query: "", status: "ranked", ranked_from: "2026-09-09",
    sort: "favourites_desc", include_nsfw: false, cursor_string: null,
  });
});

it("searches the original title without retaining conflicting keywords or dropping filters", () => {
  const query = { ...createDefaultSearchQuery("mania"), query: "old", title: "other", title_unicode: "旧标题", status: "loved", stars_min: 4, cursor_string: "next" };
  const result = searchSameTitle(query, { id: 1, title: "Song", title_unicode: "原名", artist: "Artist", creator: "Mapper", status: "ranked" });
  expect(result).toMatchObject({ query: "", title: "", title_unicode: "原名", ruleset: "mania", status: "loved", stars_min: 4, sort: "relevance_desc", cursor_string: null });
  expect(onlineResultsTitle(result)).toBe("搜索结果");
  expect(onlineResultsTitle(createDefaultSearchQuery("osu"))).toBe("在线谱面");
  expect(onlineResultsTitle({ ...query, query: "", title: "", title_unicode: "", mapper: "Someone" })).toBe("搜索结果");
});
