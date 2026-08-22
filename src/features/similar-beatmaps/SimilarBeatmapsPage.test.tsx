import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, useLocation } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";

import { desktopApi } from "../../shared/lib/tauri";
import type {
  OsuSimilarityQueryResponse,
  OsuSimilarityRecommendationResponse,
  SimilarityIndexStatus,
} from "../../shared/types/osu";
import { ModeProvider } from "../../app/ModeContext";
import { SimilarBeatmapsPage } from "./SimilarBeatmapsPage";
import { settingsQueryKey } from "../settings/api";
import { defaultSimilarityPreferences } from "./defaults";

vi.mock("./SimilarityRadar", () => ({
  SimilarityRadar: ({
    comparison,
  }: {
    comparison?: Record<string, number> | null;
  }) => <div data-testid={comparison ? "comparison-radar" : "target-radar"} />,
}));

const unconfigured: SimilarityIndexStatus = {
  ruleset: "osu",
  state: "unconfigured",
  directory: null,
  message: "尚未配置本地相似谱面索引。",
  record_count: null,
  analyzer_version: null,
  normalization_version: null,
  algorithm_id: null,
  data_cutoff_at: null,
  supports_dynamic_weighting: false,
  records_by_key_count: {},
};

const ready: SimilarityIndexStatus = {
  ruleset: "osu",
  state: "ready",
  directory: "D:/private-index",
  message: "本地索引已就绪。",
  record_count: 3,
  analyzer_version: 4,
  normalization_version: 1,
  algorithm_id: "five-dimension-slider-rosu-reading-v4",
  data_cutoff_at: 1_785_140_308,
  supports_dynamic_weighting: false,
  records_by_key_count: {},
};

const feature = {
  aim: 0.7,
  speed: 0.6,
  reading: 0.8,
  slider: 0.2,
  overlap: 0.5,
};

const base = {
  bpm: 180,
  ar: 9,
  od: 8,
  cs: 4,
  hp: 6,
  length_seconds: 120,
  object_count: 500,
  object_density: 4.1,
  circle_ratio: 0.6,
  slider_ratio: 0.38,
  spinner_ratio: 0.02,
  max_combo: 800,
};

const response: OsuSimilarityQueryResponse = {
  ruleset: "osu",
  target: {
    ruleset: "osu",
    beatmap_id: 10,
    beatmapset_id: 1,
    artist: "Reference",
    title: "Target",
    version: "Insane",
    creator: "Mapper",
    online_url: "https://osu.ppy.sh/b/10",
    star_rating: 6.1,
    difficulty: feature,
    base,
    source: "index",
    analyzer_version: 4,
    normalization_version: 1,
  },
  results: [
    {
      ruleset: "osu",
      beatmap_id: 20,
      beatmapset_id: 2,
      artist: "Signal",
      title: "Candidate",
      version: "Another",
      creator: "Other Mapper",
      online_url: "https://osu.ppy.sh/b/20",
      star_rating: 6.2,
      difficulty: { ...feature, reading: 0.75 },
      base: { ...base, bpm: 182 },
      final_distance: 0.04,
      difficulty_distance: 0.03,
      base_distance: 0.08,
    },
  ],
  dynamic_profile: {
    target_star_rating: 6.1,
    candidate_min_section: 57,
    candidate_max_section: 65,
    stats_min_section: 57,
    stats_max_section: 65,
    sample_count: 240,
    mean: { ...feature, slider: 0.1 },
    stddev: { aim: 0.1, speed: 0.1, reading: 0.1, slider: 0.05, overlap: 0.1 },
    delta: { aim: 0, speed: 0, reading: 0, slider: 0.1, overlap: 0 },
    z_score: { aim: 0, speed: 0, reading: 0, slider: 2, overlap: 0 },
    weights: { aim: 0.25, speed: 0.25, reading: 0.25, slider: 1.75, overlap: 0.25 },
    parameter_mean: { ar: 9, cs: 4, od: 8.5 },
    parameter_stddev: { ar: 0.5, cs: 0.4, od: 0.6 },
    parameter_delta: { ar: 0.2, cs: 0, od: 0.1 },
    parameter_z_score: { ar: 0.4, cs: 0, od: 0.17 },
    parameter_group_z_score: 0.25,
    parameter_weight: 0.44,
    fallback_reason: null,
  },
};

const recommendationResponse: OsuSimilarityRecommendationResponse = {
  ruleset: "osu",
  kind: "recent",
  seed_count: 20,
  skipped_seed_count: 1,
  results: [
    {
      ...response.results[0],
      recommended_by: response.target,
    },
  ],
  dynamic_profiles: [{ ...response.dynamic_profile!, seed_beatmap_id: 10 }],
};

function LocationProbe() {
  const location = useLocation();
  return <output data-testid="location">{location.pathname + location.search}</output>;
}

function renderPage(advancedEnabled = false) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  client.setQueryData(settingsQueryKey, {
    similarity_preferences: { ...defaultSimilarityPreferences, advanced_enabled: advancedEnabled },
    preview_volume: 65,
    default_beatmap_download_provider: "hinai",
    include_video_in_beatmap_downloads: true,
    beatmap_download_directory: null,
  });
  return render(
    <QueryClientProvider client={client}>
      <ModeProvider>
        <MemoryRouter initialEntries={["/online/similar"]}>
          <SimilarBeatmapsPage />
          <LocationProbe />
        </MemoryRouter>
      </ModeProvider>
    </QueryClientProvider>,
  );
}

afterEach(() => {
  vi.restoreAllMocks();
  localStorage.clear();
});

describe("SimilarBeatmapsPage", () => {
  it("treats an unconfigured private index as a normal empty state", async () => {
    vi.spyOn(desktopApi, "getSimilarityIndexStatus").mockResolvedValue(unconfigured);

    renderPage();

    expect(await screen.findByText("本地索引未配置")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "查找相似谱面" })).not.toBeInTheDocument();
    expect(screen.queryByText(/下载|获取方式|文件大小/)).not.toBeInTheDocument();
  });

  it("configures a user-selected directory and immediately enables search", async () => {
    const user = userEvent.setup();
    vi.spyOn(desktopApi, "getSimilarityIndexStatus").mockResolvedValue(unconfigured);
    vi.spyOn(desktopApi, "chooseDirectory").mockResolvedValue("D:/private-index");
    const configure = vi
      .spyOn(desktopApi, "configureSimilarityIndex")
      .mockResolvedValue(ready);

    renderPage();
    await user.click(await screen.findByRole("button", { name: "选择索引目录" }));

    expect(configure).toHaveBeenCalledWith("osu", "D:/private-index");
    expect(await screen.findByText("索引已就绪")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "查找相似谱面" })).toBeDisabled();
  });

  it("supports advanced parameters, result comparison and the online deep link", async () => {
    const user = userEvent.setup();
    vi.spyOn(desktopApi, "getSimilarityIndexStatus").mockResolvedValue(ready);
    const query = vi
      .spyOn(desktopApi, "querySimilarBeatmaps")
      .mockResolvedValue(response);
    vi.spyOn(desktopApi, "chooseBeatmapDownloadDirectory").mockResolvedValue("D:/downloads");
    const download = vi.spyOn(desktopApi, "downloadOnlineBeatmapsets").mockResolvedValue({
      destination: "D:/downloads",
      total: 1,
      completed: 1,
      skipped: 0,
      failed: 0,
      cancelled: false,
      failures: [],
    });

    renderPage(true);
    const input = await screen.findByLabelText("Beatmap ID 或 osu! 链接");
    await user.type(input, "https://osu.ppy.sh/beatmaps/10");
    await user.click(screen.getByRole("button", { name: "展开高级参数" }));
    expect(screen.getByLabelText("结果数量")).toHaveValue("50");
    expect(screen.queryByRole("tab", { name: "动态" })).not.toBeInTheDocument();
    expect(screen.getByLabelText("Speed 权重")).toHaveValue("2");
    await user.click(screen.getByRole("button", { name: "查找相似谱面" }));

    expect(await screen.findByText("Signal - Candidate")).toBeInTheDocument();
    expect(query).toHaveBeenCalledWith(
      expect.objectContaining({
        source: {
          kind: "beatmap_id",
          value: "https://osu.ppy.sh/beatmaps/10",
        },
        weighting: expect.objectContaining({ mode: "manual", parameter_weight: 1 }),
        result_limit: 50,
      }),
    );
    expect(screen.queryByText("动态权重档案")).not.toBeInTheDocument();
    expect(screen.getByTestId("comparison-radar")).toBeInTheDocument();

    await user.click(screen.getByLabelText(/Candidate/));
    expect(download).toHaveBeenCalledWith({
      destination: "D:/downloads",
      provider: "hinai",
      overwrite: false,
      include_video: true,
      items: [{ beatmapset_id: 2, artist: "Signal", title: "Candidate" }],
    });

    await user.click(screen.getByText("在在线谱面中查看"));
    expect(screen.getByTestId("location")).toHaveTextContent(
      "/online/beatmaps?beatmapset=2&beatmap=20",
    );
  });

  it("uses fixed weighting when the index lacks dynamic statistics", async () => {
    const user = userEvent.setup();
    vi.spyOn(desktopApi, "getSimilarityIndexStatus").mockResolvedValue({
      ...ready,
      supports_dynamic_weighting: false,
    });

    renderPage(true);
    await screen.findByText("索引已就绪");
    const advancedToggle = screen.getByRole("button", { name: /(展开|收起)高级参数/ });
    if (advancedToggle.textContent?.includes("展开")) await user.click(advancedToggle);
    expect(screen.getByLabelText("Aim 权重")).toBeInTheDocument();
    expect(screen.queryByRole("tab", { name: "动态" })).not.toBeInTheDocument();
  });

  it("accepts a local osu file through the desktop picker", async () => {
    const user = userEvent.setup();
    vi.spyOn(desktopApi, "getSimilarityIndexStatus").mockResolvedValue(ready);
    vi.spyOn(desktopApi, "chooseSimilarityBeatmapFile").mockResolvedValue(
      "D:/maps/reference.osu",
    );

    renderPage();
    await user.click(await screen.findByRole("tab", { name: "本地 .osu" }));
    await user.click(screen.getByRole("button", { name: "选择文件" }));

    expect(screen.getByLabelText("osu!standard 谱面文件")).toHaveValue(
      "D:/maps/reference.osu",
    );
    expect(screen.getByRole("button", { name: "查找相似谱面" })).toBeEnabled();
  });

  it("recommends from recent plays or BP and shows the nearest source beatmap", async () => {
    const user = userEvent.setup();
    vi.spyOn(desktopApi, "getSimilarityIndexStatus").mockResolvedValue(ready);
    const recommend = vi
      .spyOn(desktopApi, "recommendSimilarBeatmaps")
      .mockImplementation(async (request) => ({
        ...recommendationResponse,
        kind: request.kind,
        results: request.seed_limit === 5 ? [] : recommendationResponse.results,
      }));

    renderPage();
    await user.click(await screen.findByRole("button", { name: "根据最近游玩推荐" }));

    expect(await screen.findByText("根据最近游玩生成")).toBeInTheDocument();
    expect(screen.getAllByText(/由 Reference - Target \[Insane\] 推荐/).length).toBeGreaterThan(0);
    expect(recommend).toHaveBeenLastCalledWith(
      expect.objectContaining({ kind: "recent", result_limit: 50 }),
    );

    await user.click(screen.getByRole("button", { name: "根据你的 BP 推荐" }));
    await waitFor(() => expect(screen.getByText("根据你的 BP 生成")).toBeInTheDocument());
    expect(recommend).toHaveBeenLastCalledWith(
      expect.objectContaining({ kind: "best", result_limit: 50 }),
    );
  });

  it("shows five recommended beatmaps at a time and switches batches", async () => {
    const user = userEvent.setup();
    vi.spyOn(desktopApi, "getSimilarityIndexStatus").mockResolvedValue(ready);
    vi.spyOn(desktopApi, "recommendSimilarBeatmaps").mockImplementation(async (request) => ({
      ...recommendationResponse,
      results: request.seed_limit === 5 ? [] : Array.from({ length: 6 }, (_, index) => ({
        ...recommendationResponse.results[0],
        beatmap_id: 20 + index,
        beatmapset_id: 200 + index,
        artist: `Artist ${index + 1}`,
        title: `Recommendation ${index + 1}`,
      })),
    }));

    renderPage();
    await user.click(await screen.findByRole("button", { name: "根据最近游玩推荐" }));

    expect(await screen.findByText("Artist 1 - Recommendation 1")).toBeInTheDocument();
    expect(screen.queryByText("Artist 6 - Recommendation 6")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "换一批" }));
    expect(screen.queryByText("Artist 1 - Recommendation 1")).not.toBeInTheDocument();
    expect(screen.getByText("Artist 6 - Recommendation 6")).toBeInTheDocument();
  });

  it("records complete batches and excludes them from the next recommendation", async () => {
    const user = userEvent.setup();
    vi.spyOn(desktopApi, "getSimilarityIndexStatus").mockResolvedValue(ready);
    const recommendations = Array.from({ length: 5 }, (_, index) => ({
      ...recommendationResponse.results[0],
      beatmap_id: 30 + index,
      beatmapset_id: 300 + index,
      title: `History ${index + 1}`,
    }));
    const recommend = vi.spyOn(desktopApi, "recommendSimilarBeatmaps").mockImplementation(async (request) => ({
      ...recommendationResponse,
      results: request.seed_limit === 5 ? [] : recommendations,
    }));

    renderPage();
    await user.click(await screen.findByRole("button", { name: "根据最近游玩推荐" }));
    expect(await screen.findByText("Signal - History 1")).toBeInTheDocument();
    await waitFor(() => expect(localStorage.getItem("opp.similarity-recommendation-history.v2")).toContain("History 5"));
    const historyButton = await screen.findByRole("button", { name: /今日推荐历史/ });

    await user.click(historyButton);
    expect(screen.getByRole("dialog", { name: "今日推荐历史" })).toHaveTextContent("Signal - History 5");
    await user.click(screen.getByRole("button", { name: "关闭今日推荐历史" }));
    await user.click(screen.getByRole("button", { name: "根据最近游玩推荐" }));

    await waitFor(() => expect(recommend).toHaveBeenCalledWith(
      expect.objectContaining({
        excluded_beatmap_ids: expect.arrayContaining(
          recommendations.map((result) => result.beatmap_id),
        ),
      }),
    ));
  });

  it("does not repeat the displayed quick batch when the deferred standard response completes", async () => {
    const user = userEvent.setup();
    vi.spyOn(desktopApi, "getSimilarityIndexStatus").mockResolvedValue(ready);
    const quickResults = Array.from({ length: 5 }, (_, index) => ({
      ...recommendationResponse.results[0],
      beatmap_id: 40 + index,
      beatmapset_id: 400 + index,
      title: `Quick ${index + 1}`,
    }));
    const freshResults = Array.from({ length: 5 }, (_, index) => ({
      ...recommendationResponse.results[0],
      beatmap_id: 50 + index,
      beatmapset_id: 500 + index,
      title: `Full ${index + 1}`,
    }));
    const quickResponse: OsuSimilarityRecommendationResponse = {
      ...recommendationResponse,
      results: quickResults,
    };
    const fullResponse: OsuSimilarityRecommendationResponse = {
      ...recommendationResponse,
      results: [...quickResults, ...freshResults],
    };
    let resolveFull!: (response: OsuSimilarityRecommendationResponse) => void;
    const deferredFull = new Promise<OsuSimilarityRecommendationResponse>((resolve) => {
      resolveFull = resolve;
    });
    vi.spyOn(desktopApi, "recommendSimilarBeatmaps").mockImplementation((request) =>
      request.seed_limit === 5
        ? Promise.resolve({ ...quickResponse, kind: request.kind })
        : deferredFull,
    );

    renderPage();
    await user.click(await screen.findByRole("button", { name: "根据最近游玩推荐" }));
    expect(await screen.findByText("Signal - Quick 1")).toBeInTheDocument();
    await waitFor(() => expect(localStorage.getItem("opp.similarity-recommendation-history.v2")).toContain("Quick 5"));

    await act(async () => resolveFull(fullResponse));

    expect(await screen.findByText("Signal - Full 1")).toBeInTheDocument();
    expect(screen.queryByText("Signal - Quick 1")).not.toBeInTheDocument();
  });

  it("applies range sliders to the recalled candidate batch without changing the query", async () => {
    const user = userEvent.setup();
    vi.spyOn(desktopApi, "getSimilarityIndexStatus").mockResolvedValue(ready);
    const query = vi.spyOn(desktopApi, "querySimilarBeatmaps").mockResolvedValue(response);

    renderPage();
    await user.click(await screen.findByRole("tab", { name: "ID / 链接" }));
    await user.type(await screen.findByLabelText("Beatmap ID 或 osu! 链接"), "10");
    await user.click(screen.getByRole("button", { name: "查找相似谱面" }));
    expect(await screen.findByText("Signal - Candidate")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /候选谱面筛选/ }));
    fireEvent.change(screen.getByLabelText("BPM 最低"), { target: { value: "400" } });

    expect(screen.queryByText("Signal - Candidate")).not.toBeInTheDocument();
    expect(query).toHaveBeenCalledTimes(1);
  });
});
