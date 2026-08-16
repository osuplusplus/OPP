import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ModeProvider } from "../../app/ModeContext";
import type {
  LocalLibrarySummary,
  LocalSourceStatus,
} from "../../shared/types/osu";
import {
  LocalAnalysisPage,
  type LocalSection,
} from "./LocalAnalysisPage";

const mocks = vi.hoisted(() => ({
  getLocalSources: vi.fn(),
  getLocalSummary: vi.fn(),
  queryLocalBeatmapSets: vi.fn(),
  queryLocalSkins: vi.fn(),
  getLocalSkinDetail: vi.fn(),
  getLocalSkinPreview: vi.fn(),
  getLocalSkinAsset: vi.fn(),
  onLocalScanProgress: vi.fn(async () => () => undefined),
}));

vi.mock("../../shared/lib/tauri", () => ({
  desktopApi: {
    ...mocks,
    chooseLocalDirectory: vi.fn(),
    setLocalSource: vi.fn(),
    resetLocalSource: vi.fn(),
    scanLocalSource: vi.fn(),
    cancelLocalScan: vi.fn(),
    getLocalBeatmapDetail: vi.fn(),
    getLocalBeatmapBackground: vi.fn(),
  },
}));

function source(client: "stable" | "lazer"): LocalSourceStatus {
  return {
    client,
    mode: "auto",
    configured_path: null,
    install_root: client === "stable" ? "D:\\osu!" : "C:\\Local\\osulazer",
    data_root: client === "stable" ? "D:\\osu!" : "C:\\Roaming\\osu",
    version: "2026",
    valid: true,
    validation_errors: [],
    capabilities: {
      beatmaps: client === "stable" ? "full" : "partial",
      difficulty: "full",
      skins: client === "stable" ? "full" : "partial",
      skin_resources: client === "stable" ? "full" : "unavailable",
      realm_index: false,
    },
    last_scanned_at: null,
  };
}

function summary(client: "stable" | "lazer"): LocalLibrarySummary {
  return {
    client,
    completeness: client === "stable" ? "complete" : "partial",
    source_root: client === "stable" ? "D:\\osu!" : "C:\\Roaming\\osu",
    scanned_at: "2026-07-25T00:00:00Z",
    beatmap_count: 10,
    beatmap_set_count: 4,
    beatmap_set_count_inferred: client === "lazer",
    skin_count: 2,
    source_file_count: 20,
    source_bytes: 1024,
    diagnostic_count: 0,
    mode_counts: { osu: 10 },
    calculation: {
      engine: "rosu-pp",
      engine_version: "4.0.1",
      engine_released_at: "2026-04-12",
      upstream_repository: "ppy/osu",
      upstream_revision: "28c846b4d9366484792e27f4729cd1afa2cdeb66",
      upstream_date: "2025-10-13",
      ruleset_versions: {
        osu: 20250306,
        taiko: 20250306,
        fruits: 20250306,
        mania: 20241007,
      },
      modifiers: "NoMod",
      performance_assumption: "满分 / 最大连击 / 0 miss",
    },
  };
}

function renderPage(section: LocalSection = "maps") {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <ModeProvider>
        <LocalAnalysisPage section={section} />
      </ModeProvider>
    </QueryClientProvider>,
  );
}

describe("LocalAnalysisPage", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.clearAllMocks();
    mocks.getLocalSources.mockResolvedValue([source("stable"), source("lazer")]);
    mocks.queryLocalBeatmapSets.mockResolvedValue({
      items: [],
      total: 0,
      offset: 0,
      limit: 40,
    });
    mocks.queryLocalSkins.mockResolvedValue({
      items: [],
      total: 0,
      offset: 0,
      limit: 100,
    });
  });

  it("shows the first-scan state for a valid source without an index", async () => {
    mocks.getLocalSummary.mockResolvedValue(null);
    renderPage();

    expect(await screen.findByText("还没有本地索引")).toBeInTheDocument();
    expect(screen.getAllByText(/扫描/).length).toBeGreaterThan(0);
  });

  it("uses the global ruleset in paged beatmap queries", async () => {
    mocks.getLocalSummary.mockResolvedValue(summary("stable"));
    renderPage();

    await waitFor(() => expect(mocks.queryLocalBeatmapSets).toHaveBeenCalled());
    expect(mocks.queryLocalBeatmapSets).toHaveBeenCalledWith(
      expect.objectContaining({
        client: "stable",
        rulesets: ["osu"],
        offset: 0,
        limit: 40,
      }),
    );
  });

  it("keeps lazer Skin browsing in its own partial-index section", async () => {
    localStorage.setItem("opp.global-client", "lazer");
    mocks.getLocalSummary.mockResolvedValue(summary("lazer"));
    renderPage("skins");

    expect(await screen.findByText("本地皮肤")).toBeInTheDocument();
    expect(await screen.findByText("Realm 索引")).toBeInTheDocument();
    await waitFor(() => expect(mocks.queryLocalSkins).toHaveBeenCalled());
    expect(mocks.queryLocalBeatmapSets).not.toHaveBeenCalled();
  });

  it("loads image and sound manifests for the selected stable Skin", async () => {
    const resource = {
      resource_id: "stable:skin:fixture",
      client: "stable" as const,
      content_hash: "fixture",
      logical_path: "Skins/Fixture/skin.ini",
    };
    const skin = {
      resource,
      completeness: "complete" as const,
      name: "Fixture Skin",
      author: "OPP",
      version: "2.7",
      section_count: 1,
      has_mania_config: false,
      resource_count: 3,
      total_bytes: 1024,
      modified_at: null,
      accent_colors: [[255, 120, 180]],
    };
    mocks.getLocalSummary.mockResolvedValue(summary("stable"));
    mocks.queryLocalSkins.mockResolvedValue({
      items: [skin],
      total: 1,
      offset: 0,
      limit: 100,
    });
    mocks.getLocalSkinDetail.mockResolvedValue({
      summary: skin,
      sections: [{ name: "General", entries: [] }],
      inventory: {
        file_count: 3,
        total_bytes: 1024,
        by_extension: { ini: 1, png: 1, wav: 1 },
      },
      notice: null,
    });
    mocks.getLocalSkinPreview.mockResolvedValue({
      skin_resource_id: resource.resource_id,
      completeness: "complete",
      images: [{
        resource_id: "image:cursor",
        kind: "image",
        name: "cursor.png",
        logical_path: "cursor.png",
        extension: "png",
        size: 128,
        category: "光标",
      }],
      sounds: [{
        resource_id: "audio:hitnormal",
        kind: "audio",
        name: "normal-hitnormal.wav",
        logical_path: "normal-hitnormal.wav",
        extension: "wav",
        size: 256,
        category: "击打音效",
      }],
    });
    mocks.getLocalSkinAsset.mockResolvedValue({
      resource_id: "image:cursor",
      kind: "image",
      mime_type: "image/png",
      data_url: "data:image/png;base64,AA==",
    });

    renderPage("skins");

    expect(
      await screen.findByRole("heading", { name: "Fixture Skin" }),
    ).toBeInTheDocument();
    await waitFor(() => expect(mocks.getLocalSkinPreview).toHaveBeenCalled());
    expect(await screen.findByAltText("cursor.png")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /音效/ })).toBeInTheDocument();
  });
});
