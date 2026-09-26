import { useState } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSettings } from "../../shared/types/osu";
import { LocalDataPanel } from "./panels/LocalDataPanel";

const mocks = vi.hoisted(() => ({
  getLocalSources: vi.fn(), getLocalSummary: vi.fn(), getLocalIndexStatus: vi.fn(),
  scanLocalSource: vi.fn(), onLocalScanProgress: vi.fn(), save: vi.fn(), configure: vi.fn(),
}));
vi.mock("../../shared/lib/tauri", () => ({ desktopApi: mocks }));
vi.mock("../local-database/DatabaseSetup", () => ({ LocalDatabaseCard: () => <div>数据库位置与索引同步</div> }));

function mount() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  function Harness() {
    const [settings, setSettings] = useState({ show_local_beatmap_presence: true, local_beatmap_presence_scope: "all", beatmap_download_directory: "D:/Downloads" } as AppSettings);
    return <LocalDataPanel settings={settings} save={async (next) => { await mocks.save(next); setSettings(next); }} busy={false} onConfigure={mocks.configure} />;
  }
  return render(<QueryClientProvider client={client}><Harness /></QueryClientProvider>);
}

describe("local data settings", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mocks.getLocalIndexStatus.mockResolvedValue({ phase: "ready", clients: {} });
    mocks.getLocalSources.mockResolvedValue(["stable", "lazer"].map((client) => ({ client, valid: true, data_root: `D:/${client}`, configured_path: null, validation_errors: [] })));
    mocks.getLocalSummary.mockResolvedValue({ beatmap_count: 10, scanned_at: "2026-09-26" });
    mocks.scanLocalSource.mockResolvedValue({ beatmap_count: 11, scanned_at: "2026-09-27" });
    mocks.onLocalScanProgress.mockResolvedValue(() => {});
  });

  it("can incrementally update an already indexed client and configure its directory", async () => {
    mount();
    expect(screen.getByText("数据库位置与索引同步")).toBeInTheDocument();
    const stable = screen.getByRole("region", { name: "osu! Stable 本地索引" });
    await userEvent.click(await within(stable).findByRole("button", { name: "更新本地索引" }));
    await waitFor(() => expect(mocks.scanLocalSource).toHaveBeenCalledWith("stable", false));
    expect(mocks.scanLocalSource).toHaveBeenCalledOnce();
    await userEvent.click(screen.getByRole("button", { name: "配置游戏目录" }));
    expect(mocks.configure).toHaveBeenCalledOnce();
  });

  it("saves presence preferences while preserving other settings", async () => {
    mount();
    await userEvent.click(screen.getByRole("switch", { name: "显示本地已有谱面" }));
    await waitFor(() => expect(mocks.save).toHaveBeenLastCalledWith(expect.objectContaining({ show_local_beatmap_presence: false, beatmap_download_directory: "D:/Downloads" })));
    await userEvent.selectOptions(screen.getByRole("combobox", { name: "本地谱面匹配客户端" }), "lazer");
    await waitFor(() => expect(mocks.save).toHaveBeenLastCalledWith(expect.objectContaining({ show_local_beatmap_presence: false, local_beatmap_presence_scope: "lazer", beatmap_download_directory: "D:/Downloads" })));
  });
});
