import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { SkinWorkshopPage } from "./SkinWorkshopPage";

const mocks = vi.hoisted(() => ({
  scanLocalSource: vi.fn(),
  skinItems: [] as Array<Record<string, unknown>>,
  previewImages: [] as Array<Record<string, unknown>>,
}));

vi.mock("../../shared/lib/tauri", () => ({
  desktopApi: {
    scanLocalSource: mocks.scanLocalSource,
  },
}));

vi.mock("../../app/ModeContext", () => ({
  useMode: () => ({ client: "stable" }),
}));

vi.mock("../local-analysis/api", () => ({
  localSkinsKey: () => ["local-skins"],
  useLocalSkinAsset: (_client: string, _skinId: string, assetId: string | null) => ({ data: assetId ? { data_url: `data:image/png;base64,${assetId}` } : null }),
  useLocalSkinDetail: () => ({ data: null }),
  useLocalSkinPreview: () => ({ data: { images: mocks.previewImages, sounds: [] } }),
  useLocalSkins: () => ({
    data: { items: mocks.skinItems },
    isLoading: false,
    refetch: vi.fn(),
  }),
}));

describe("SkinWorkshopPage", () => {
  it("使用紧凑工具栏展示本地皮肤库", () => {
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });

    render(
      <QueryClientProvider client={queryClient}>
        <SkinWorkshopPage />
      </QueryClientProvider>,
    );

    expect(screen.getByRole("heading", { name: "本地皮肤" })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "搜索 Skin" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "选择要编辑的 Skin" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "打开 .osk" })).toBeInTheDocument();
  });

  it("在皮肤卡片中展示预览入口并移除占位式大按钮", () => {
    mocks.skinItems = [{
      resource: { resource_id: "skin:1", client: "stable", content_hash: "hash", logical_path: "C:\\osu!\\Skins\\Refined" },
      completeness: "complete",
      name: "Refined",
      author: "Mapper",
      version: "2.0",
      section_count: 8,
      has_mania_config: true,
      resource_count: 128,
      total_bytes: 4096,
      modified_at: null,
      accent_colors: [[92, 225, 230], [255, 106, 167]],
    }];
    mocks.previewImages = [
      { resource_id: "cursor", kind: "image", name: "cursor.png", logical_path: "cursor.png", extension: "png", size: 128, category: "gameplay" },
      { resource_id: "circle", kind: "image", name: "hitcircle.png", logical_path: "hitcircle.png", extension: "png", size: 256, category: "gameplay" },
      { resource_id: "overlay", kind: "image", name: "hitcircleoverlay.png", logical_path: "hitcircleoverlay.png", extension: "png", size: 256, category: "gameplay" },
    ];
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(<QueryClientProvider client={queryClient}><SkinWorkshopPage /></QueryClientProvider>);

    expect(screen.getByRole("button", { name: "预览 Refined" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "在本地打开 Refined" })).toBeInTheDocument();
    expect(screen.getByAltText("Refined 光标预览")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "进入预览" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "本地打开" })).not.toBeInTheDocument();
    mocks.skinItems = [];
    mocks.previewImages = [];
  });

  it("rescans the local source before refreshing the skin library", async () => {
    mocks.scanLocalSource.mockResolvedValue(undefined);
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });

    render(
      <QueryClientProvider client={queryClient}>
        <SkinWorkshopPage />
      </QueryClientProvider>,
    );

    fireEvent.click(screen.getByRole("button", { name: "重新扫描 Skin" }));

    await waitFor(() => {
      expect(mocks.scanLocalSource).toHaveBeenCalledWith("stable", false);
    });
  });
});
