import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { LocalDatabaseStatus } from "../../shared/types/osu";
import { LocalDatabaseCard, LocalDatabaseGate } from "./DatabaseSetup";

const mocks = vi.hoisted(() => ({
  getLocalLibraryStorageStatus: vi.fn(),
  migrateLocalLibraryDatabase: vi.fn(),
  getLocalDatabaseStatus: vi.fn(),
  initializeLocalDatabase: vi.fn(),
  retryLocalDatabase: vi.fn(),
  locateLocalDatabase: vi.fn(),
  chooseLocalDirectory: vi.fn(),
}));
vi.mock("../../shared/lib/tauri", () => ({ desktopApi: mocks }));

const unconfigured: LocalDatabaseStatus = {
  phase: "unconfigured", directory: null, recommended_directory: "C:\\OPP\\datasets",
  database_uuid: null, schema_version: null, can_initialize: true, error: null,
};
const ready: LocalDatabaseStatus = {
  ...unconfigured, phase: "ready", directory: "D:\\用户数据库", database_uuid: "library-uuid",
  schema_version: 1, can_initialize: false,
};
const unavailable: LocalDatabaseStatus = {
  ...ready, phase: "error", schema_version: null,
  error: { code: "DATABASE_FILE_UNAVAILABLE", message: "数据库磁盘不可用" },
};

function mount(card = false) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  return render(<QueryClientProvider client={client}>
    {card ? <LocalDatabaseCard /> : <LocalDatabaseGate><div>登录与新手引导入口</div></LocalDatabaseGate>}
  </QueryClientProvider>);
}

describe("local database startup and recovery", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mocks.getLocalLibraryStorageStatus.mockResolvedValue([]);
    mocks.migrateLocalLibraryDatabase.mockResolvedValue([]);
    mocks.getLocalDatabaseStatus.mockResolvedValue(unconfigured);
    mocks.initializeLocalDatabase.mockResolvedValue(ready);
    mocks.retryLocalDatabase.mockResolvedValue(ready);
    mocks.locateLocalDatabase.mockResolvedValue(ready);
    mocks.chooseLocalDirectory.mockResolvedValue(null);
  });
  afterEach(cleanup);

  it("shows migrated client counts and synchronizes cached indexes without choosing a new directory", async () => {
    mocks.getLocalDatabaseStatus.mockResolvedValue(ready);
    mocks.getLocalLibraryStorageStatus.mockResolvedValue([{ client: "stable", storage: "json", revision: "2026-09-26", beatmap_count: 12, entry_count: 20, error: "数据库写入失败" }]);
    mocks.migrateLocalLibraryDatabase.mockResolvedValue([{ client: "stable", storage: "database", revision: "2026-09-26", beatmap_count: 12, entry_count: 20, error: null }]);
    mount(true);
    expect(await screen.findByText(/Stable · JSON 回退 · 12 张谱面 · 20 个资源条目/)).toBeInTheDocument();
    expect(await screen.findByRole("alert")).toHaveTextContent("数据库写入失败");
    await userEvent.click(screen.getByRole("button", { name: "导入／同步现有索引" }));
    expect(await screen.findByText(/Stable · 数据库 · 12 张谱面/)).toBeInTheDocument();
    expect(mocks.chooseLocalDirectory).not.toHaveBeenCalled();
    expect(mocks.initializeLocalDatabase).not.toHaveBeenCalled();
  });

  it("waits for status before mounting authentication or onboarding", async () => {
    let resolve!: (status: LocalDatabaseStatus) => void;
    mocks.getLocalDatabaseStatus.mockReturnValue(new Promise<LocalDatabaseStatus>((done) => { resolve = done; }));
    mount();
    expect(screen.getByText("正在打开本地数据库…")).toBeInTheDocument();
    expect(screen.queryByText("登录与新手引导入口")).not.toBeInTheDocument();
    resolve(unconfigured);
    expect(await screen.findByRole("dialog", { name: "设置本地数据库保存位置" })).toBeInTheDocument();
    expect(screen.queryByText("登录与新手引导入口")).not.toBeInTheDocument();
  });

  it("skips only this startup and prompts again on the next mount", async () => {
    const user = userEvent.setup();
    const view = mount();
    await user.click(await screen.findByRole("button", { name: "稍后设置" }));
    expect(screen.getByText("登录与新手引导入口")).toBeInTheDocument();
    expect(mocks.initializeLocalDatabase).not.toHaveBeenCalled();
    view.unmount();
    mount();
    expect(await screen.findByRole("dialog", { name: "设置本地数据库保存位置" })).toBeInTheDocument();
  });

  it.each(["close", "escape"])("treats %s as a session-only skip", async (method) => {
    const user = userEvent.setup();
    mount();
    await screen.findByRole("dialog");
    if (method === "close") await user.click(screen.getByRole("button", { name: "关闭" }));
    else await user.keyboard("{Escape}");
    expect(await screen.findByText("登录与新手引导入口")).toBeInTheDocument();
    expect(mocks.initializeLocalDatabase).not.toHaveBeenCalled();
  });

  it("keeps the setup dialog when the directory picker is cancelled", async () => {
    const user = userEvent.setup();
    mount();
    await user.click(await screen.findByRole("button", { name: "选择文件夹" }));
    await waitFor(() => expect(mocks.chooseLocalDirectory).toHaveBeenCalledWith(unconfigured.recommended_directory, "选择 OPP 本地数据库保存目录"));
    expect(mocks.initializeLocalDatabase).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(screen.queryByText("登录与新手引导入口")).not.toBeInTheDocument();
  });

  it("initializes the recommended directory and leaves later startups without a dialog", async () => {
    const user = userEvent.setup();
    const view = mount();
    await user.click(await screen.findByRole("button", { name: "使用推荐位置" }));
    expect(await screen.findByText("登录与新手引导入口")).toBeInTheDocument();
    expect(mocks.initializeLocalDatabase).toHaveBeenCalledWith(unconfigured.recommended_directory);
    view.unmount();
    mocks.getLocalDatabaseStatus.mockResolvedValue(ready);
    mount();
    expect(await screen.findByText("登录与新手引导入口")).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("serializes UI actions and prevents dismissal while initializing", async () => {
    let resolve!: (status: LocalDatabaseStatus) => void;
    mocks.initializeLocalDatabase.mockReturnValue(new Promise<LocalDatabaseStatus>((done) => { resolve = done; }));
    const user = userEvent.setup();
    mount();
    await user.click(await screen.findByRole("button", { name: "使用推荐位置" }));
    expect(screen.getByRole("button", { name: "选择文件夹" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "稍后设置" })).toBeDisabled();
    await user.keyboard("{Escape}");
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    resolve(ready);
    expect(await screen.findByText("登录与新手引导入口")).toBeInTheDocument();
  });

  it("shows initialization errors and permits retry with another chosen folder", async () => {
    mocks.initializeLocalDatabase.mockRejectedValueOnce({ code: "DATABASE_READ_ONLY", message: "目录不可写" });
    mocks.chooseLocalDirectory.mockResolvedValue(ready.directory);
    const user = userEvent.setup();
    mount();
    await user.click(await screen.findByRole("button", { name: "使用推荐位置" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("目录不可写");
    await user.click(screen.getByRole("button", { name: "选择文件夹" }));
    expect(await screen.findByText("登录与新手引导入口")).toBeInTheDocument();
    expect(mocks.initializeLocalDatabase).toHaveBeenLastCalledWith(ready.directory);
  });

  it("retries the configured library without offering fresh initialization", async () => {
    mocks.getLocalDatabaseStatus.mockResolvedValue(unavailable);
    const user = userEvent.setup();
    mount();
    expect(await screen.findByRole("alert")).toHaveTextContent("数据库磁盘不可用");
    expect(screen.queryByRole("button", { name: "使用推荐位置" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "重试打开" }));
    expect(await screen.findByText("登录与新手引导入口")).toBeInTheDocument();
    expect(mocks.initializeLocalDatabase).not.toHaveBeenCalled();
  });

  it("settings recover through locate and show ready status without move controls", async () => {
    mocks.getLocalDatabaseStatus.mockResolvedValue(unavailable);
    mocks.chooseLocalDirectory.mockResolvedValue("E:\\原数据库");
    const user = userEvent.setup();
    mount(true);
    await user.click(await screen.findByRole("button", { name: "重新定位原数据库" }));
    expect(mocks.locateLocalDatabase).toHaveBeenCalledWith("E:\\原数据库");
    expect(await screen.findByText(/数据库可用 · 结构版本 1/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "重新定位原数据库" })).not.toBeInTheDocument();
    expect(mocks.initializeLocalDatabase).not.toHaveBeenCalled();
  });
});
