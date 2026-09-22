import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";

import { ModeProvider } from "../../app/ModeContext";
import { desktopApi } from "../../shared/lib/tauri";
import type { LocalBeatmapSetSummary, LocalBeatmapSummary, OsuSimilarityQueryResponse, OsuSimilarityRecommendationResponse, SimilarityIndexStatus } from "../../shared/types/osu";
import { settingsQueryKey } from "../settings/api";
import { defaultSimilarityPreferences } from "./defaults";
import { resetStandardSimilaritySessionForTests, SimilarBeatmapsPage } from "./SimilarBeatmapsPage";

vi.mock("./SimilarityRadar", () => ({ SimilarityRadar: () => <div data-testid="comparison-radar" /> }));

const ready: SimilarityIndexStatus = { ruleset: "osu", state: "ready", directory: "D:/index", message: "ready", record_count: 100, analyzer_version: 4, normalization_version: 4, algorithm_id: "v4", data_cutoff_at: null, supports_dynamic_weighting: false, records_by_key_count: null };
const base = { bpm: 180, ar: 9, od: 8, cs: 4, hp: 6, length_seconds: 120, object_count: 500, object_density: 4, circle_ratio: .6, slider_ratio: .38, spinner_ratio: .02, max_combo: 700 };
const difficulty = { aim: .7, speed: .6, reading: .5, slider: .4, overlap: .3 };
const target = { ruleset: "osu" as const, beatmap_id: 10, beatmapset_id: 1, artist: "Reference", title: "Target", version: "Insane", creator: "Mapper", online_url: "", star_rating: 6.1, base, difficulty };
const result = (id: number, title: string, bpm = 182) => ({ ...target, beatmap_id: id, beatmapset_id: id + 100, artist: "Signal", title, base: { ...base, bpm }, final_distance: .05, difficulty_distance: .03, base_distance: .02 });
const response: OsuSimilarityQueryResponse = { ruleset: "osu", target: { ...target, source: "index", analyzer_version: 4, normalization_version: 4 }, results: [result(20, "Candidate One"), result(21, "Candidate Two")], dynamic_profile: null };
const recommendation: OsuSimilarityRecommendationResponse = { ruleset: "osu", kind: "recent", seed_count: 5, skipped_seed_count: 0, results: [result(30, "Recommended One"), result(31, "Recommended Two")].map((item) => ({ ...item, recommended_by: target })), dynamic_profiles: [] };

function renderPage() {
  localStorage.setItem("opp.global-ruleset", "osu");
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  client.setQueryData(settingsQueryKey, { similarity_preferences: defaultSimilarityPreferences, preview_volume: 65, default_beatmap_download_provider: "hinai", include_video_in_beatmap_downloads: true, beatmap_download_directory: "D:/downloads" });
  return render(<QueryClientProvider client={client}><ModeProvider><MemoryRouter initialEntries={["/online/similar"]}><SimilarBeatmapsPage /></MemoryRouter></ModeProvider></QueryClientProvider>);
}

afterEach(() => { vi.restoreAllMocks(); localStorage.clear(); resetStandardSimilaritySessionForTests(); });

describe("standard similarity workspace", () => {
  it("reserves a fresh dataset generation only for explicit revalidation", async () => {
    vi.spyOn(desktopApi, "getSimilarityIndexStatus").mockResolvedValue(ready);
    const configure = vi.spyOn(desktopApi, "configureSimilarityIndex").mockResolvedValue(ready);
    renderPage();
    const button = await screen.findByRole("button", { name: "重新校验" });
    expect(configure).not.toHaveBeenCalled();
    fireEvent.click(button);
    await waitFor(() => expect(configure).toHaveBeenCalledWith("osu", "D:/index"));
  });

  it("resolves an ID from the unified search and shows one candidate at a time", async () => {
    const user = userEvent.setup();
    vi.spyOn(desktopApi, "getSimilarityIndexStatus").mockResolvedValue(ready);
    const query = vi.spyOn(desktopApi, "querySimilarBeatmaps").mockResolvedValue(response);
    renderPage();
    await user.type(await screen.findByLabelText("搜索本地谱面、Beatmap ID 或链接"), "10");
    await user.click(screen.getByRole("button", { name: "查询相似" }));
    expect(await screen.findByText("Candidate One")).toBeInTheDocument();
    expect(screen.queryByText("Candidate Two")).not.toBeInTheDocument();
    expect(query).toHaveBeenCalledWith(expect.objectContaining({ source: { kind: "beatmap_id", value: "10" } }));
    expect(screen.getByText("1 / 2")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /下一首/ }));
    expect(screen.getByText("Candidate Two")).toBeInTheDocument();
    expect(screen.getByTestId("comparison-radar")).toBeInTheDocument();
  });

  it("searches the local library after debounce and resolves the selected local path", async () => {
    const user = userEvent.setup();
    vi.spyOn(desktopApi, "getSimilarityIndexStatus").mockResolvedValue(ready);
    vi.spyOn(desktopApi, "querySimilarBeatmaps").mockResolvedValue(response);
    const local: LocalBeatmapSummary = { resource: { client: "stable", resource_id: "local-1", logical_path: "Songs/map.osu", content_hash: "abc" }, set_key: "set-1", set_grouping_inferred: false, beatmap_id: null, beatmap_set_id: null, title: "Local Song", title_unicode: "", artist: "Artist", artist_unicode: "", creator: "Mapper", difficulty_name: "Local Diff", ruleset: "osu", format_version: 14, stars: 5.2, max_pp: null, max_combo: 500, bpm: 180, length_ms: 120000, object_count: 400, cs: 4, ar: 9, od: 8, hp: 6, average_nps: 3, peak_nps: 7, modified_at: null, analysis_status: "ready" };
    const localSet: LocalBeatmapSetSummary = { set_key: "set-1", completeness: "complete", grouping_inferred: false, beatmap_set_id: null, title: "Local Song", title_unicode: "", artist: "Artist", artist_unicode: "", creators: ["Mapper"], min_stars: 5.2, max_stars: 5.2, bpm: 180, length_ms: 120000, object_count: 400, modified_at: null, background_resource_id: null, difficulties: [local] };
    vi.spyOn(desktopApi, "queryLocalBeatmapSets").mockResolvedValue({ items: [localSet], total: 1, offset: 0, limit: 8 });
    vi.spyOn(desktopApi, "getLocalBeatmapPath").mockResolvedValue("D:/osu/Songs/map.osu");
    const query = vi.spyOn(desktopApi, "querySimilarBeatmaps");
    renderPage();
    await user.type(await screen.findByLabelText("搜索本地谱面、Beatmap ID 或链接"), "Local Song");
    await user.click(await screen.findByRole("option", { name: /Local Song/ }));
    await user.click(await screen.findByRole("option", { name: /Local Diff/ }));
    await waitFor(() => expect(query).toHaveBeenCalledWith(expect.objectContaining({ source: { kind: "local_file", path: "D:/osu/Songs/map.osu" } })));
  });

  it("keeps quick recommendations visible, appends the full response and records only viewed entries", async () => {
    const user = userEvent.setup();
    vi.spyOn(desktopApi, "getSimilarityIndexStatus").mockResolvedValue(ready);
    vi.spyOn(desktopApi, "recommendSimilarBeatmaps").mockImplementation(async (request) => request.seed_limit ? { ...recommendation, results: recommendation.results.slice(0, 1) } : recommendation);
    renderPage();
    await user.click(await screen.findByRole("button", { name: /根据最近游玩推荐/ }));
    expect(await screen.findByText("Recommended One")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByText("1 / 2")).toBeInTheDocument());
    await waitFor(() => expect(localStorage.getItem("opp.similarity-recommendation-history.v2")).toContain("Recommended One"));
    const stored = localStorage.getItem("opp.similarity-recommendation-history.v2") ?? "";
    expect(stored).toContain("Recommended One");
    expect(stored).not.toContain("Recommended Two");
    await user.click(screen.getByRole("button", { name: /下一首/ }));
    await waitFor(() => expect(localStorage.getItem("opp.similarity-recommendation-history.v2")).toContain("Recommended Two"));
  });

  it("filters returned candidates without issuing another similarity query", async () => {
    const user = userEvent.setup();
    vi.spyOn(desktopApi, "getSimilarityIndexStatus").mockResolvedValue(ready);
    const query = vi.spyOn(desktopApi, "querySimilarBeatmaps").mockResolvedValue(response);
    renderPage();
    await user.type(await screen.findByLabelText("搜索本地谱面、Beatmap ID 或链接"), "10");
    await user.click(screen.getByRole("button", { name: "查询相似" }));
    await screen.findByText("Candidate One");
    await user.click(screen.getByRole("button", { name: /候选谱面筛选/ }));
    fireEvent.change(screen.getByLabelText("BPM 最低"), { target: { value: "400" } });
    expect(screen.getByText("Candidate One")).toBeInTheDocument();
    expect(screen.queryByText("没有符合条件的候选谱面")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "应用筛选" }));
    expect(await screen.findByText("没有符合条件的候选谱面")).toBeInTheDocument();
    expect(query).toHaveBeenCalledTimes(1);
  });

  it("shows request failures as a dismissible message instead of page content", async () => {
    const user = userEvent.setup();
    vi.spyOn(desktopApi, "getSimilarityIndexStatus").mockResolvedValue(ready);
    vi.spyOn(desktopApi, "querySimilarBeatmaps").mockRejectedValue(new Error("osu! 请求失败（404 Not Found）"));
    renderPage();
    await user.type(await screen.findByLabelText("搜索本地谱面、Beatmap ID 或链接"), "10");
    await user.click(screen.getByRole("button", { name: "查询相似" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("404 Not Found");
    await user.click(screen.getByRole("button", { name: "关闭消息" }));
    await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
  });
});
