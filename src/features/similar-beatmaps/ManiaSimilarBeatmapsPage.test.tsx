import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";

import { ModeProvider } from "../../app/ModeContext";
import { desktopApi } from "../../shared/lib/tauri";
import type { ManiaPatternView, ManiaSimilarityQueryResponse, ManiaSimilarityRecommendationResponse, SimilarityIndexStatus } from "../../shared/types/osu";
import { settingsQueryKey } from "../settings/api";
import { defaultSimilarityPreferences } from "./defaults";
import { SimilarBeatmapsPage } from "./SimilarBeatmapsPage";
import { resetManiaSimilaritySessionForTests } from "./ManiaSimilarBeatmapsPage";

vi.mock("./SimilarityRadar", () => ({ SimilarityRadar: ({ patternView, patternViewComparison }: { patternView?: ManiaPatternView | null; patternViewComparison?: ManiaPatternView | null }) => <div data-testid="mania-radar" data-pattern={Boolean(patternView)} data-pattern-comparison={Boolean(patternViewComparison)} /> }));

const patternView: ManiaPatternView = {
  category: "Shield", mode_tag: "Mix", coverage: [.52, .31, .12, .66, .48, .09],
  bars: [{ pattern: "Coordination", amount: 158000, relative: .66, specific_types: [["Shield", .092]] }],
  subtypes: [["Shield", .092]], ln_note_ratio: .234, intensity: [6.4, 12.1, 5.2, 18.5], temporal: [.42, .18, .12], duration_seconds: 120, sv_amount: 0,
};

const ready: SimilarityIndexStatus = { ruleset: "mania", state: "ready", directory: "D:/mania", message: "ready", record_count: 100, analyzer_version: 1, normalization_version: 1, algorithm_id: "mania-v1", data_cutoff_at: null, supports_dynamic_weighting: false, records_by_key_count: { 4: 60, 6: 20, 7: 20 } };
const difficulty = { speed: .7, hand_stream: .6, jack: .5, chordjack: .4, technical: .6, stamina: .7, long_note: .2, course: .5 };
const style = { stream: .7, chordstream: .4, jacks: .3, coordination: .5, density: .6, wildcard: .2, chord_rate: .3, large_chord_rate: .1, rotation_rate: .4, anchor_rate: .2, rhythm_entropy: .5, transition_entropy: .5, ln_note_ratio: .1, hold_occupancy: .1, hybrid_row_ratio: .1, peak_to_sustain_gap: .3 };
const base = { bpm: 180, length_seconds: 120, active_length_seconds: 110, note_count: 800, row_count: 650, avg_nps: 7, peak_nps: 12, break_density: .1, sv_change_rate: 0 };
const target = { ruleset: "mania" as const, beatmap_id: 3001, beatmapset_id: 700, artist: "Reference", title: "Key Target", version: "4K Another", creator: "Mapper", online_url: "https://osu.ppy.sh/beatmaps/3001", pattern_view: null, key_count: 4 as const, family: "rc" as const, pattern: "stream" as const, difficulty, style, base, difficulty_percentile: .78, difficulty_band: 7, game_mod: "NM" as const };
const result = (id: number, keyCount: 4 | 6 | 7, title: string, bpm = 180) => ({ ...target, beatmap_id: id, beatmapset_id: id + 100, title, version: `${keyCount}K Another`, key_count: keyCount, base: { ...base, bpm }, final_distance: .054, distance_components: { skill: .04, pattern: .06, structure: .08, difficulty: .03, context: .05 } });
const response: ManiaSimilarityQueryResponse = { ruleset: "mania", target: { ...target, source: "index", analyzer_version: 1, normalization_version: 1 }, results: [result(3101, 4, "Stream One"), result(3102, 4, "Stream Two", 220)] };
const recommendation: ManiaSimilarityRecommendationResponse = { ruleset: "mania", kind: "recent", seed_count: 10, skipped_seed_count: 0, groups: [
  { key_count: 4, seed_count: 5, results: [result(3201, 4, "4K One"), result(3202, 4, "4K Two")].map((item) => ({ ...item, recommended_by: target })) },
  { key_count: 6, seed_count: 5, results: [{ ...result(3301, 6, "6K One"), recommended_by: { ...target, key_count: 6 as const } }] },
  { key_count: 7, seed_count: 0, results: [] },
] };

function renderPage() {
  localStorage.setItem("opp.global-ruleset", "mania");
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  client.setQueryData(settingsQueryKey, { similarity_preferences: defaultSimilarityPreferences, preview_volume: 65, default_beatmap_download_provider: "hinai", include_video_in_beatmap_downloads: true, beatmap_download_directory: "D:/downloads" });
  return render(<QueryClientProvider client={client}><ModeProvider><MemoryRouter initialEntries={["/online/similar"]}><SimilarBeatmapsPage /></MemoryRouter></ModeProvider></QueryClientProvider>);
}

afterEach(() => { vi.restoreAllMocks(); localStorage.clear(); resetManiaSimilaritySessionForTests(); });

describe("mania similarity workspace", () => {
  it.each([[true, true], [true, false], [false, true], [false, false]])("compares patterns only when both sides have records (reference=%s, candidate=%s)", async (hasSource, hasCandidate) => {
    const user = userEvent.setup();
    vi.spyOn(desktopApi, "getSimilarityIndexStatus").mockResolvedValue(ready);
    vi.spyOn(desktopApi, "querySimilarBeatmaps").mockResolvedValue({
      ...response,
      target: { ...response.target, pattern_view: hasSource ? patternView : null },
      results: response.results.map((item) => ({ ...item, pattern_view: hasCandidate ? { ...patternView, category: "Jumpstream", mode_tag: "RC", ln_note_ratio: .05 } : null })),
    });
    renderPage();
    await user.type(await screen.findByLabelText("搜索本地谱面、Beatmap ID 或链接"), "3001");
    await user.click(screen.getByRole("button", { name: "查询相似" }));
    await screen.findByText("Stream One");
    expect(screen.getByTestId("mania-radar")).toHaveAttribute("data-pattern", String(hasSource && hasCandidate));
    expect(screen.getByTestId("mania-radar")).toHaveAttribute("data-pattern-comparison", String(hasSource && hasCandidate));
    if (hasSource) {
      const panel = within(screen.getByRole("region", { name: "参考谱面键型" }));
      expect(panel.getByText("Mix · Shield")).toBeInTheDocument();
      expect(panel.getByText("RC 76.6%")).toBeInTheDocument();
    } else expect(screen.queryByRole("region", { name: "参考谱面键型" })).not.toBeInTheDocument();
    if (hasCandidate) {
      const panel = within(screen.getByRole("region", { name: "候选谱面键型" }));
      expect(panel.getByText("RC · Jumpstream")).toBeInTheDocument();
      expect(panel.getByText("LN 5.0%")).toBeInTheDocument();
    } else expect(screen.queryByRole("region", { name: "候选谱面键型" })).not.toBeInTheDocument();
  });

  it("keeps source-free results browsable and in history without sending synthetic IDs online", async () => {
    const user = userEvent.setup();
    vi.spyOn(desktopApi, "getSimilarityIndexStatus").mockResolvedValue(ready);
    const localSource = { ...target, beatmap_id: 2 ** 48, beatmapset_id: 900001, online_url: "" };
    const localRecommendation: ManiaSimilarityRecommendationResponse = {
      ...recommendation,
      groups: [{ key_count: 4, seed_count: 1, results: [1, 2].map((id) => ({ ...result(2 ** 48 + id, 4, `Local ${id}`), online_url: "", beatmapset_id: 900001 + id, recommended_by: localSource })) }],
    };
    vi.spyOn(desktopApi, "recommendSimilarBeatmaps").mockResolvedValue(localRecommendation);
    const metadata = vi.spyOn(desktopApi, "getOnlineBeatmapset");
    const background = vi.spyOn(desktopApi, "getOnlineBeatmapBackground");
    const download = vi.spyOn(desktopApi, "downloadOnlineBeatmapsets");
    renderPage();
    await user.click(await screen.findByRole("button", { name: /根据最近游玩推荐/ }));
    await screen.findByText("Local 1");
    for (const name of ["试听", "下载", "预览", "收藏"]) {
      const button = screen.getByRole("button", { name });
      expect(button).toBeDisabled();
      await user.click(button);
    }
    await waitFor(() => expect(localStorage.getItem("opp.similarity-recommendation-history.v2")).toContain("Local 1"));
    await user.click(screen.getByRole("button", { name: /下一首/ }));
    expect(await screen.findByText("Local 2")).toBeInTheDocument();
    await waitFor(() => expect(localStorage.getItem("opp.similarity-recommendation-history.v2")).toContain("Local 2"));
    await user.click(screen.getByRole("button", { name: "历史" }));
    const history = within(screen.getByRole("dialog", { name: "今日 Mania 推荐历史" }));
    const entry = history.getByRole("button", { name: /Local 1/ });
    expect(entry).toBeDisabled();
    await user.click(entry);
    expect(screen.getByRole("dialog", { name: "今日 Mania 推荐历史" })).toBeInTheDocument();
    expect(metadata).not.toHaveBeenCalled();
    expect(background).not.toHaveBeenCalled();
    expect(download).not.toHaveBeenCalled();
  });

  it.each(["recent", "best"] as const)("keeps %s quick/full requests and cached results isolated by Mod pool", async (kind) => {
    const user = userEvent.setup();
    vi.spyOn(desktopApi, "getSimilarityIndexStatus").mockResolvedValue(ready);
    const recommend = vi.spyOn(desktopApi, "recommendSimilarBeatmaps").mockImplementation(async (request) => ({ ...recommendation, kind: request.kind }));
    renderPage();
    const buttonName = kind === "recent" ? /根据最近游玩推荐/ : /根据你的 BP 推荐/;
    await screen.findByRole("button", { name: buttonName });
    for (const [index, mod] of ["NM", "DT", "HT", "mixed"].entries()) {
      if (mod === "mixed") await user.click(screen.getByLabelText("NM / DT / HT 多 Mod 混池"));
      else await user.click(screen.getByRole("button", { name: mod }));
      await user.click(screen.getByRole("button", { name: buttonName }));
      await screen.findByText("4K One");
      await waitFor(() => expect(recommend).toHaveBeenCalledTimes((index + 1) * 2));
      const pool = mod === "mixed" ? ["NM", "DT", "HT"] : [mod];
      expect(recommend.mock.calls[index * 2][0]).toEqual(expect.objectContaining({ kind, candidate_mods: pool, seed_limit: 5, result_limit: 5 }));
      expect(recommend.mock.calls[index * 2 + 1][0]).toEqual(expect.objectContaining({ kind, candidate_mods: pool }));
      expect(recommend.mock.calls[index * 2 + 1][0].seed_limit).toBeUndefined();
      expect(recommend.mock.calls[index * 2 + 1][0].result_limit).toBeGreaterThan(5);
      await waitFor(() => expect(localStorage.getItem("opp.similarity-recommendation-history.v2")).toContain("4K One"));
      await user.click(screen.getByRole("button", { name: "返回搜索首页" }));
      await screen.findByRole("button", { name: buttonName });
      localStorage.removeItem("opp.similarity-recommendation-history.v2");
    }
    await user.click(screen.getByLabelText("NM / DT / HT 多 Mod 混池"));
    await user.click(screen.getByRole("button", { name: "NM" }));
    await user.click(screen.getByRole("button", { name: buttonName }));
    await screen.findByText("4K One");
    expect(recommend).toHaveBeenCalledTimes(8);
  });

  it("queries with Mania mods and navigates one result at a time", async () => {
    const user = userEvent.setup();
    vi.spyOn(desktopApi, "getSimilarityIndexStatus").mockResolvedValue(ready);
    const query = vi.spyOn(desktopApi, "querySimilarBeatmaps").mockResolvedValue(response);
    renderPage();
    await user.click(await screen.findByRole("button", { name: "DT" }));
    await user.click(screen.getByLabelText("NM / DT / HT 多 Mod 混池"));
    await user.type(screen.getByLabelText("搜索本地谱面、Beatmap ID 或链接"), "3001");
    await user.click(screen.getByRole("button", { name: "查询相似" }));
    expect(await screen.findByText("Stream One")).toBeInTheDocument();
    expect(screen.queryByText("Stream Two")).not.toBeInTheDocument();
    expect(query).toHaveBeenCalledWith(expect.objectContaining({ ruleset: "mania", source: { kind: "beatmap_id", value: "3001" }, target_mod: "DT", candidate_mods: ["NM", "DT", "HT"] }));
    await user.click(screen.getByRole("button", { name: /下一首/ }));
    expect(screen.getByText("Stream Two")).toBeInTheDocument();
    expect(screen.getByTestId("mania-radar")).toBeInTheDocument();
  });

  it("keeps key-count groups and records each viewed recommendation", async () => {
    const user = userEvent.setup();
    vi.spyOn(desktopApi, "getSimilarityIndexStatus").mockResolvedValue(ready);
    vi.spyOn(desktopApi, "recommendSimilarBeatmaps").mockImplementation(async (request) => request.seed_limit ? { ...recommendation, groups: recommendation.groups.map((group) => ({ ...group, results: group.results.slice(0, 1) })) } : recommendation);
    renderPage();
    await user.click(await screen.findByRole("button", { name: /根据最近游玩推荐/ }));
    expect(await screen.findByText("4K One")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("tab", { name: /6K · 1/ })).toBeInTheDocument());
    await waitFor(() => expect(localStorage.getItem("opp.similarity-recommendation-history.v2")).toContain("4K One"));
    expect(localStorage.getItem("opp.similarity-recommendation-history.v2")).not.toContain("4K Two");
    await user.click(screen.getByRole("tab", { name: /6K · 1/ }));
    expect(screen.getByText("6K One")).toBeInTheDocument();
    await waitFor(() => expect(localStorage.getItem("opp.similarity-recommendation-history.v2")).toContain("6K One"));
  });

  it("filters returned Mania candidates and reports the before/after count", async () => {
    const user = userEvent.setup();
    vi.spyOn(desktopApi, "getSimilarityIndexStatus").mockResolvedValue(ready);
    const query = vi.spyOn(desktopApi, "querySimilarBeatmaps").mockResolvedValue(response);
    renderPage();
    await user.type(await screen.findByLabelText("搜索本地谱面、Beatmap ID 或链接"), "3001");
    await user.click(screen.getByRole("button", { name: "查询相似" }));
    await screen.findByText("Stream One");
    await user.click(screen.getByRole("button", { name: /候选谱面筛选/ }));
    fireEvent.change(screen.getByLabelText("BPM 最低"), { target: { value: "200" } });
    expect(screen.getByText("Stream One")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "应用筛选" }));
    expect(screen.getByText("Stream Two")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /候选谱面筛选/ }));
    expect(screen.getByText(/当前 1 \/ 2/)).toBeInTheDocument();
    expect(query).toHaveBeenCalledTimes(1);
  });
});
