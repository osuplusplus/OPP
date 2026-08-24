import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { SkinWorkshopPage } from "./SkinWorkshopPage";

const mocks = vi.hoisted(() => ({
  scanLocalSource: vi.fn(),
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
  useLocalSkinDetail: () => ({ data: null }),
  useLocalSkins: () => ({
    data: { items: [] },
    isLoading: false,
    refetch: vi.fn(),
  }),
}));

describe("SkinWorkshopPage", () => {
  it("使用紧凑入口展示当前直接编辑工作区", () => {
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });

    render(
      <QueryClientProvider client={queryClient}>
        <SkinWorkshopPage />
      </QueryClientProvider>,
    );

    expect(screen.getByRole("heading", { name: "Skin Workshop" })).toBeInTheDocument();
    const entryHeading = screen.getByRole("heading", { name: "选择要编辑的 Skin" });
    expect(entryHeading).toBeInTheDocument();
    expect(entryHeading.closest("[data-slot='card']")).toHaveClass("p-5");
    expect(entryHeading.closest("[data-slot='card']")).not.toHaveClass("p-7");
    expect(screen.getByRole("button", { name: "打开 .osk" })).toBeInTheDocument();
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
