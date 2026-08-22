import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ModeProvider } from "../../app/ModeContext";
import { desktopApi } from "../../shared/lib/tauri";
import { ReplayRenderPage } from "./ReplayRenderPage";

function renderPage() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={queryClient}><ModeProvider><MemoryRouter><ReplayRenderPage /></MemoryRouter></ModeProvider></QueryClientProvider>);
}

describe("ReplayRenderPage", () => {
  beforeEach(() => {
    window.localStorage.clear();
    vi.spyOn(desktopApi, "getDanserStatus").mockResolvedValue({ available: true, executable_path: "C:\\danser\\danser-cli.exe", ffmpeg_available: true, profiles: ["default"], message: "ready" });
    vi.spyOn(desktopApi, "listGameMedia").mockResolvedValue([]);
    vi.spyOn(desktopApi, "getDanserRenderQueue").mockResolvedValue([]);
  });

  it("defaults to live preview, switches providers, and remembers the choice", async () => {
    const user = userEvent.setup();
    renderPage();
    // 新用户默认进入实时预览，仍可切换到本地与在线渲染。
    expect(screen.getByRole("tab", { name: "实时预览" })).toHaveAttribute("aria-selected", "true");
    expect(await screen.findByText("预览区域")).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "本地 Danser" }));
    expect(screen.getByRole("tab", { name: "本地 Danser" })).toHaveAttribute("aria-selected", "true");
    expect(await screen.findByText("本地运行环境")).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "在线 o!rdr" }));
    expect(screen.getByRole("tab", { name: "在线 o!rdr" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByText(/o!rdr 只接收回放文件/)).toBeInTheDocument();
    expect(window.localStorage.getItem("opp:replay-render-provider")).toBe("ordr");
  });

  it("shows the supported Danser version requirement", async () => {
    renderPage();
    expect(await screen.findByText("版本要求：Danser 0.11.x")).toBeInTheDocument();
  });
});
