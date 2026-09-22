import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { expect, it, vi } from "vitest";
import { desktopApi } from "../../shared/lib/tauri";
import { CollectionsPage } from "./CollectionsPage";

vi.mock("../../shared/lib/tauri", async (original) => {
  const real = await original<typeof import("../../shared/lib/tauri")>();
  return { ...real, desktopApi: { ...real.desktopApi,
    listCollectionSummaries: vi.fn().mockResolvedValue({ folders: [], sources: [] }),
    refreshCollectionSummaries: vi.fn().mockRejectedValue({ code: "COLLECTION_MANAGER_NOT_CONFIGURED", message: "未配置 CollectionManager shim" }),
    onBeatmapDownloadProgress: vi.fn().mockResolvedValue(() => undefined),
  } };
});

it("reads the explicitly selected lazer source and exposes dependency errors", async () => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<QueryClientProvider client={client}><MemoryRouter><CollectionsPage /></MemoryRouter></QueryClientProvider>);
  await screen.findByText("还没有收藏夹");
  await userEvent.click(screen.getByRole("button", { name: "读取 lazer" }));
  expect(desktopApi.refreshCollectionSummaries).toHaveBeenCalledWith("lazer");
  expect(await screen.findByText("未配置 CollectionManager shim")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "补齐并写回 Stable" })).toBeInTheDocument();
  client.clear();
});
