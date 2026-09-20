import { Suspense } from "react";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Link, MemoryRouter, Outlet } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";
import { AppRoutes } from "./AppRoutes";
import { SettingsLayout } from "../features/settings/SettingsLayout";

vi.mock("./AppShell", () => ({ AppShell: () => <><Link to="/settings">打开设置</Link><Link to="/tools">打开工具</Link><Suspense fallback="加载页面"><Outlet /></Suspense></> }));
vi.mock("../features/online-beatmaps/OnlineBeatmapsPage", () => ({ OnlineBeatmapsPage: () => <input aria-label="背景搜索" defaultValue="" /> }));
vi.mock("../features/settings/SettingsPage", () => ({ SettingsPage: () => <SettingsLayout activeCategory="general" onCategoryChange={() => {}}><p>设置内容</p></SettingsLayout> }));
vi.mock("../features/tools/ToolCategoryPages", () => ({
  GameToolsPage: () => <><p>游戏工具内容</p><input aria-label="工具输入" /></>,
  BeatmapToolsPage: () => <p>谱面工具内容</p>,
  SystemToolsPage: () => <p>系统工具内容</p>,
  LiveToolsPage: () => <p>直播工具内容</p>,
}));

describe("route dialogs", () => {
  it("keeps the background mounted and preserves search through settings and tool category changes", async () => {
    const user = userEvent.setup();
    render(<MemoryRouter initialEntries={["/online/beatmaps"]}><AppRoutes /></MemoryRouter>);
    const search = await screen.findByLabelText("背景搜索");
    await user.type(search, "retained search");
    await user.click(screen.getByRole("link", { name: "打开设置" }));
    expect(await screen.findByRole("dialog", { name: "设置" })).toBeVisible();
    expect(search).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(screen.getByLabelText("背景搜索")).toBe(search);
    expect(search).toHaveValue("retained search");
    await user.click(screen.getByRole("link", { name: "打开工具" }));
    await screen.findByText("游戏工具内容");
    const dialog = screen.getByRole("dialog");
    const toolInput = screen.getByLabelText("工具输入");
    await user.type(toolInput, "keep tool draft");
    await user.click(screen.getByRole("button", { name: "系统与文件" }));
    await screen.findByText("系统工具内容");
    expect(screen.getByRole("dialog")).toBe(dialog);
    expect(search).toBeVisible();
    await user.click(screen.getByRole("button", { name: "游戏与设备" }));
    expect(screen.getByLabelText("工具输入")).toBe(toolInput);
    expect(toolInput).toHaveValue("keep tool draft");
    await user.click(screen.getByRole("button", { name: "关闭工具" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(screen.getByLabelText("背景搜索")).toBe(search);
    expect(search).toHaveValue("retained search");
  });

  it("can close a settings deep link without a previous history entry", async () => {
    render(<MemoryRouter initialEntries={["/settings"]}><AppRoutes /></MemoryRouter>);
    await screen.findByRole("dialog", { name: "设置" });
    await userEvent.setup().click(screen.getByRole("button", { name: "关闭设置" }));
    expect(await screen.findByRole("textbox", { name: "背景搜索" })).toBeVisible();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});
