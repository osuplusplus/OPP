import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  getBeatmapHubAuthStatus: vi.fn(),
  getAuthStatus: vi.fn(),
  listCollections: vi.fn(),
  getBeatmapHubProfile: vi.fn(),
  previewBeatmapHubPack: vi.fn(),
  getOnlineBeatmapset: vi.fn(),
}));

vi.mock("../../shared/lib/tauri", () => ({ desktopApi: api, isTauri: () => true }));

import { BeatmapHubPage } from "./BeatmapHubPage";

function renderPage() {
  return render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><BeatmapHubPage /></QueryClientProvider>);
}

describe("BeatmapHubPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    api.getAuthStatus.mockResolvedValue({ credentials_configured: true, connected: true, client_id: "1", callback_url: "http://localhost", user_id: 1, username: "L1rics" });
    api.listCollections.mockResolvedValue({ folders: [], sources: [] });
  });

  it("offers independent profile creation and device linking", async () => {
    api.getBeatmapHubAuthStatus.mockResolvedValue({ has_identity: false, connected: false, device_name: "TEST-PC" });
    renderPage();
    expect(await screen.findByRole("heading", { name: "连接 BeatmapHub" })).toBeInTheDocument();
    expect(screen.getByDisplayValue("L1rics")).toBeInTheDocument();
    expect(screen.getByDisplayValue("TEST-PC")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "链接已有档案" }));
    expect(screen.getByPlaceholderText("粘贴旧设备生成的 43 位链接码")).toBeInTheDocument();
  });

  it("opens a pack and presents preview-before-import actions", async () => {
    api.getBeatmapHubAuthStatus.mockResolvedValue({ has_identity: true, connected: true, display_name: "Player", device_name: "PC" });
    api.getBeatmapHubProfile.mockResolvedValue({ user: { id: "user", display_name: "Player" }, current_device_id: "device", devices: [] });
    api.previewBeatmapHubPack.mockResolvedValue({ pack: { id: "7K3N9A", title: "Tech Pack", description: "Practice", owner: { id: "user", display_name: "Player" }, beatmapset_ids: [123], manifest_hash: "hash", rating: { average: 4.5, count: 2 }, viewer: { rating: 5, favorited: true, can_edit: true }, created_at: new Date().toISOString(), updated_at: new Date().toISOString() }, locally_available_ids: [], missing_ids: [123] });
    api.getOnlineBeatmapset.mockResolvedValue({ id: 123, title: "Map", artist: "Artist", creator: "Mapper", status: "ranked", beatmaps: [] });
    renderPage();
    const input = await screen.findByPlaceholderText("BPH-7K3N9A");
    await userEvent.type(input, "7K3N9A");
    await userEvent.click(screen.getAllByRole("button", { name: "打开" }).slice(-1)[0]);
    expect(await screen.findByRole("heading", { name: "Tech Pack" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /确认导入/ })).toBeInTheDocument();
    expect(screen.getAllByText("已收藏").slice(-1)[0]).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "删除曲包" })).toBeInTheDocument();
  });
});
