import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { LocalLibrarySummary, LocalSourceStatus } from "../shared/types/osu";
import { GlobalContextBar } from "./GlobalContextBar";
import { ModeProvider } from "./ModeContext";

const mocks = vi.hoisted(() => ({
  getLocalSources: vi.fn(),
  getLocalSummary: vi.fn(),
  scanLocalSource: vi.fn(),
  onLocalScanProgress: vi.fn(async () => () => undefined),
  getGameStatus: vi.fn(async () => ({ clients: [] })),
  onGameStatusChanged: vi.fn(async () => () => undefined),
}));

vi.mock("../shared/lib/tauri", () => ({
  desktopApi: mocks,
}));

function source(valid = true): LocalSourceStatus {
  return {
    client: "stable",
    mode: "auto",
    configured_path: null,
    install_root: valid ? "D:\\osu!" : null,
    data_root: valid ? "D:\\osu!" : null,
    version: null,
    valid,
    validation_errors: valid ? [] : ["未找到 osu! 数据目录"],
    capabilities: {
      beatmaps: "full",
      difficulty: "full",
      skins: "full",
      skin_resources: "full",
      realm_index: false,
    },
    last_scanned_at: null,
  };
}

function summary(): LocalLibrarySummary {
  return {
    client: "stable",
    completeness: "complete",
    source_root: "D:\\osu!",
    scanned_at: "2026-08-16T00:00:00Z",
    beatmap_count: 10,
    beatmap_set_count: 4,
    beatmap_set_count_inferred: false,
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
      upstream_revision: "revision",
      upstream_date: "2025-10-13",
      ruleset_versions: { osu: 20250306 },
      modifiers: "NoMod",
      performance_assumption: "满分",
    },
  };
}

function renderBar(path = "/tools") {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter initialEntries={[path]}>
        <ModeProvider>
          <GlobalContextBar />
        </ModeProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("GlobalContextBar local scan action", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.clearAllMocks();
    mocks.getLocalSources.mockResolvedValue([source()]);
    mocks.getLocalSummary.mockResolvedValue(null);
    mocks.scanLocalSource.mockResolvedValue(summary());
  });

  it("can start the first local scan from a non-local page", async () => {
    const user = userEvent.setup();
    renderBar("/tools");

    const button = await screen.findByRole("button", { name: "扫描本地数据" });
    await user.click(button);

    await waitFor(() => {
      expect(mocks.scanLocalSource).toHaveBeenCalledWith("stable", false);
    });
  });

  it("hides the scan action when the current client already has an index", async () => {
    mocks.getLocalSummary.mockResolvedValue(summary());
    renderBar("/online/beatmaps");

    await waitFor(() => expect(mocks.getLocalSummary).toHaveBeenCalled());
    expect(screen.queryByRole("button", { name: "扫描本地数据" })).not.toBeInTheDocument();
  });

  it("offers data-source configuration instead of a failing scan", async () => {
    mocks.getLocalSources.mockResolvedValue([source(false)]);
    renderBar("/data");

    expect(await screen.findByRole("button", { name: "配置数据源" })).toBeInTheDocument();
  });

  it("uses the matching label for nested and feature routes", () => {
    mocks.getLocalSummary.mockResolvedValue(summary());
    renderBar("/beatmaphub");
    expect(screen.getByText("BeatmapHub")).toBeInTheDocument();
  });
});
