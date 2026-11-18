import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { expect, it, vi } from "vitest";
import { desktopApi } from "../../shared/lib/tauri";
import type { CollectionRefreshPreview } from "../../shared/types/osu";
import { CollectionsPage } from "./CollectionsPage";

vi.mock("../../shared/lib/tauri", async (original) => {
  const real = await original<typeof import("../../shared/lib/tauri")>();
  return { ...real, desktopApi: { ...real.desktopApi,
    listCollectionSummaries: vi.fn().mockResolvedValue({ folders: [], sources: [] }),
    previewCollectionRefresh: vi.fn().mockRejectedValue({ code: "COLLECTION_SOURCE_UNAVAILABLE", message: "未配置 osu!lazer 数据目录" }),
    refreshCollectionSummaries: vi.fn().mockResolvedValue({ folders: [], sources: [] }),
    onBeatmapDownloadProgress: vi.fn().mockResolvedValue(() => undefined),
  } };
});

it("reads the explicitly selected lazer source and exposes source errors", async () => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<QueryClientProvider client={client}><MemoryRouter><CollectionsPage /></MemoryRouter></QueryClientProvider>);
  await screen.findByText("还没有收藏夹");
  await userEvent.click(screen.getByRole("button", { name: "读取 lazer" }));
  expect(desktopApi.previewCollectionRefresh).toHaveBeenCalledWith("lazer");
  expect(await screen.findByText("未配置 osu!lazer 数据目录")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "补齐并写回 Stable" })).toBeInTheDocument();
  client.clear();
});

it("confirms the diff dialog before mirroring the stable side into OPP", async () => {
  const diff: CollectionRefreshPreview = {
    client: "stable",
    has_changes: true,
    additions: [{ name: "FromGame", source: "stable", entry_count: 3, entries_added: 0, entries_removed: 0, pending_overwrite: false }],
    updates: [],
    removals: [{ name: "WrittenBefore", source: "opp", entry_count: 5, entries_added: 0, entries_removed: 0, pending_overwrite: false }],
    preserved: [],
  };
  vi.mocked(desktopApi.previewCollectionRefresh).mockResolvedValueOnce(diff);
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<QueryClientProvider client={client}><MemoryRouter><CollectionsPage /></MemoryRouter></QueryClientProvider>);
  await screen.findByText("还没有收藏夹");
  await userEvent.click(screen.getByRole("button", { name: "读取 Stable" }));
  expect(await screen.findByText("读取 osu!stable 收藏夹")).toBeInTheDocument();
  expect(screen.getByText("FromGame")).toBeInTheDocument();
  expect(screen.getByText("WrittenBefore")).toBeInTheDocument();
  // 未确认前不执行覆盖。
  expect(desktopApi.refreshCollectionSummaries).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole("button", { name: "覆盖 OPP 收藏夹" }));
  expect(desktopApi.refreshCollectionSummaries).toHaveBeenCalledWith("stable");
  expect(await screen.findByText("已按 osu!stable 侧覆盖 OPP 收藏夹：新增 1、更新 0、删除 1。")).toBeInTheDocument();
  client.clear();
});

it("applies a no-diff read directly without the dialog", async () => {
  vi.mocked(desktopApi.previewCollectionRefresh).mockResolvedValueOnce({
    client: "stable", has_changes: false, additions: [], updates: [], removals: [], preserved: [],
  });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<QueryClientProvider client={client}><MemoryRouter><CollectionsPage /></MemoryRouter></QueryClientProvider>);
  await screen.findByText("还没有收藏夹");
  await userEvent.click(screen.getByRole("button", { name: "读取 Stable" }));
  expect(await screen.findByText("已读取 osu!stable 收藏夹，与游戏侧一致。")).toBeInTheDocument();
  expect(desktopApi.refreshCollectionSummaries).toHaveBeenCalledWith("stable");
  client.clear();
});

it("filters the collection grid by stable or lazer source", async () => {
  const user = userEvent.setup();
  const folder = (id: string, name: string, source: "stable" | "lazer" | "opp") => ({ id, name, creator: "op", source, read_only: false, pending_write: false, entry_count: 1, missing_count: 0, beatmapset_count: 1, revision: 1 });
  vi.mocked(desktopApi.listCollectionSummaries).mockResolvedValue({
    folders: [folder("s1", "Stable 包", "stable"), folder("l1", "Lazer 包", "lazer"), folder("o1", "OPP 自建", "opp")],
    sources: [],
  });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<QueryClientProvider client={client}><MemoryRouter><CollectionsPage /></MemoryRouter></QueryClientProvider>);
  await screen.findByText("Stable 包");
  expect(screen.getByText("Lazer 包")).toBeInTheDocument();
  // 只看 lazer：stable 与 OPP 自建都隐藏。
  await user.selectOptions(screen.getByRole("combobox", { name: "收藏夹来源" }), "lazer");
  expect(screen.getByText("Lazer 包")).toBeInTheDocument();
  expect(screen.queryByText("Stable 包")).not.toBeInTheDocument();
  expect(screen.queryByText("OPP 自建")).not.toBeInTheDocument();
  // 切回 stable 后 lazer 隐藏。
  await user.selectOptions(screen.getByRole("combobox", { name: "收藏夹来源" }), "stable");
  expect(screen.getByText("Stable 包")).toBeInTheDocument();
  expect(screen.queryByText("Lazer 包")).not.toBeInTheDocument();
  await user.selectOptions(screen.getByRole("combobox", { name: "收藏夹来源" }), "all");
  client.clear();
});
