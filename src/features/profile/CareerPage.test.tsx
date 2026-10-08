import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ModeProvider, useMode } from "../../app/ModeContext";
import { desktopApi } from "../../shared/lib/tauri";
import { CareerPage } from "./CareerPage";
import { CareerHeatmap } from "./CareerHeatmap";
import { buildPlayActivity, fillCareerDays } from "./careerModel";
import { careerFixtureDays, careerFixtureDetail, careerFixtureProfile, careerFixtureToday } from "./careerFixtures.test-data";

// Chart rendering and pointer selection are covered by the browser check; these tests exercise the real query boundary.
vi.mock("./CareerTrend", () => ({ CareerTrend: ({ onSelect }: { onSelect: (date: string) => void }) => <button onClick={() => onSelect("2026-10-04")}>选择趋势真实点</button> }));

function ModeChange() { const { setRuleset } = useMode(); return <button onClick={() => setRuleset("mania")}>切换 mania</button>; }
function renderPage() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(<QueryClientProvider client={queryClient}><ModeProvider><CareerPage /><ModeChange /></ModeProvider></QueryClientProvider>);
  return queryClient;
}

beforeEach(() => {
  window.localStorage.clear();
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(new Date("2026-10-08T12:00:00+08:00"));
  vi.spyOn(desktopApi, "getOwnProfile").mockResolvedValue({ data: careerFixtureProfile, fetched_at: "2026-10-08T04:00:00Z", stale: false });
  vi.spyOn(desktopApi, "getCareerStatus").mockResolvedValue({ configured: true, path: null, snapshot_count: 8, latest_date: careerFixtureToday, last_error: null });
  vi.spyOn(desktopApi, "getCareerCalendar").mockImplementation(async (ruleset, start, end) => ({ ruleset, start_date: start, end_date: end, days: careerFixtureDays.filter((day) => day.date >= start && day.date <= end) }));
  vi.spyOn(desktopApi, "getCareerDay").mockImplementation(async (ruleset, date) => ({ ...careerFixtureDetail(date), ruleset }));
  vi.spyOn(desktopApi, "captureCareerSnapshot").mockResolvedValue({ local_date: careerFixtureToday, status: "captured", created: false, fetched_at: null, profile_available: true, scores_available: true, media_available: true, message: "记录已更新" });
  vi.spyOn(desktopApi, "clearCareerHistory").mockResolvedValue();
  vi.spyOn(desktopApi, "openExternal").mockResolvedValue();
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.useRealTimers(); });

describe("CareerPage", () => {
  it("keeps period summary stable while selecting dates and distinguishes missing from no events", async () => {
    const user = userEvent.setup();
    renderPage();
    const summary = await screen.findByRole("region", { name: "期间总览" });
    expect(summary).toHaveTextContent("5180.00");
    expect(summary).toHaveTextContent("上升 500 名");
    const before = summary.textContent;
    await user.click(screen.getByRole("button", { name: "选择趋势真实点" }));
    await waitFor(() => expect(within(screen.getByRole("region", { name: "当天变化" })).getByRole("heading", { name: /2026-10-04/ })).toBeInTheDocument());
    expect(summary.textContent).toBe(before);
    await user.click(screen.getByRole("button", { name: "2026-10-02 · 新增游玩 0 次" }));
    expect(await screen.findByText("这一天有记录，没有 BP、奖章或本地媒体变化。")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "2026-10-03 · 未采集" }));
    expect(await screen.findByText("这一天没有保存记录")).toBeInTheDocument();
    expect(summary.textContent).toBe(before);
    expect(screen.getByRole("button", { name: "2026-10-09 · 未来日期" })).toBeDisabled();
  });

  it("switches year/month across boundaries and resets day selection on mode change", async () => {
    const user = userEvent.setup(); renderPage();
    await screen.findByRole("region", { name: "期间总览" });
    await user.click(screen.getByRole("button", { name: "年" }));
    await waitFor(() => expect(desktopApi.getCareerCalendar).toHaveBeenCalledWith("osu", "2025-12-01", "2026-12-31"));
    await user.click(screen.getByRole("button", { name: "上一年" }));
    await waitFor(() => expect(desktopApi.getCareerCalendar).toHaveBeenCalledWith("osu", "2024-12-01", "2025-12-31"));
    await user.click(screen.getByRole("button", { name: "月" }));
    await waitFor(() => expect(desktopApi.getCareerCalendar).toHaveBeenCalledWith("osu", "2024-12-01", "2025-01-31"));
    await user.click(screen.getByRole("button", { name: "回到今天" }));
    await screen.findByRole("button", { name: "2026-10-02 · 新增游玩 0 次" });
    await user.click(screen.getByRole("button", { name: "2026-10-02 · 新增游玩 0 次" }));
    await user.click(screen.getByRole("button", { name: "切换 mania" }));
    await waitFor(() => expect(desktopApi.getCareerDay).toHaveBeenCalledWith("mania", "2026-10-08"));
    expect(desktopApi.getCareerDay).not.toHaveBeenCalledWith("mania", "2026-10-02");
  });

  it("shows the score name, PP and position and opens the official beatmap through the adapter", async () => {
    const user = userEvent.setup(); renderPage();
    const open = await screen.findByRole("button", { name: "打开谱面 Signal Garden" });
    const detail = screen.getByRole("region", { name: "当天变化" });
    expect(detail).toHaveTextContent("Signal Garden");
    expect(detail).toHaveTextContent("320.50");
    expect(detail).toHaveTextContent("#12");
    await user.click(open);
    expect(desktopApi.openExternal).toHaveBeenCalledWith("https://osu.ppy.sh/beatmaps/123");
  });

  it("keeps the historical period when changing modes but resets the selected day", async () => {
    const user = userEvent.setup(); renderPage();
    await screen.findByRole("region", { name: "期间总览" });
    await user.click(screen.getByRole("button", { name: "上个月" }));
    await user.click(await screen.findByRole("button", { name: "2026-09-02 · 未采集" }));
    await user.click(screen.getByRole("button", { name: "切换 mania" }));
    await waitFor(() => expect(desktopApi.getCareerCalendar).toHaveBeenCalledWith("mania", "2026-08-01", "2026-09-30"));
    await waitFor(() => expect(desktopApi.getCareerDay).toHaveBeenCalledWith("mania", "2026-09-30"));
    expect(desktopApi.getCareerDay).not.toHaveBeenCalledWith("mania", "2026-09-02");
  });

  it("shows saved BP events even when no profile statistics were captured", async () => {
    const day = { ...careerFixtureDays[7], status: "partial", stats: null };
    vi.mocked(desktopApi.getCareerCalendar).mockImplementation(async (ruleset, start, end) => ({ ruleset, start_date: start, end_date: end, days: [day] }));
    vi.mocked(desktopApi.getCareerDay).mockImplementation(async (ruleset, date) => ({ ...careerFixtureDetail(date), ruleset, status: "partial", stats: null, previous_stats: null }));
    renderPage();
    expect(await screen.findByText("Signal Garden")).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "当天变化" })).toHaveTextContent("部分数据未采集成功");
    expect(screen.queryByText("还没有历史记录")).not.toBeInTheDocument();
  });

  it("reports capture failure, retries, and invalidates all periods and details after clearing", async () => {
    const user = userEvent.setup(); const client = renderPage();
    await screen.findByRole("region", { name: "期间总览" });
    client.setQueryData(["career-calendar", "mania", "old", "range"], { days: [] });
    client.setQueryData(["career-day", "mania", "2026-10-01"], { date: "2026-10-01" });
    vi.mocked(desktopApi.captureCareerSnapshot).mockRejectedValueOnce(new Error("更新失败测试"));
    await user.click(screen.getByRole("button", { name: "更新记录" }));
    expect(await screen.findByText("更新失败测试")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "重试更新" }));
    expect(await screen.findByText("记录已更新")).toBeInTheDocument();
    expect(client.getQueryState(["career-calendar", "mania", "old", "range"])?.isInvalidated).toBe(true);
    expect(client.getQueryState(["career-day", "mania", "2026-10-01"])?.isInvalidated).toBe(true);
    await user.click(screen.getByLabelText("更多"));
    await user.click(screen.getByRole("button", { name: "清空历史" }));
    expect(desktopApi.clearCareerHistory).not.toHaveBeenCalled();
    vi.mocked(desktopApi.clearCareerHistory).mockRejectedValueOnce(new Error("删除失败测试"));
    await user.click(screen.getByRole("button", { name: "清空全部历史" }));
    expect(await screen.findByText(/清空失败：删除失败测试/)).toBeInTheDocument();
    vi.mocked(desktopApi.getCareerCalendar).mockImplementation(async (ruleset, start, end) => ({ ruleset, start_date: start, end_date: end, days: [] }));
    await user.click(screen.getByRole("button", { name: "清空全部历史" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(await screen.findByText("还没有历史记录")).toBeInTheDocument();
  });

  it("offers retries for calendar and detail requests without turning errors into empty history", async () => {
    const user = userEvent.setup();
    vi.mocked(desktopApi.getCareerCalendar).mockRejectedValueOnce(new Error("日历请求错误"));
    vi.mocked(desktopApi.getCareerDay).mockRejectedValueOnce(new Error("详情请求错误"));
    renderPage();
    expect(await screen.findByText("生涯记录加载失败")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "重试生涯记录" }));
    expect(await screen.findByText("当天详情加载失败")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "重试当天详情" }));
    await waitFor(() => expect(within(screen.getByRole("region", { name: "当天变化" })).getByText("BP 变化")).toBeInTheDocument());
  });
});

describe("CareerHeatmap keyboard", () => {
  it("moves focus with arrows and selects with Enter, stopping before future dates", async () => {
    const user = userEvent.setup(); const select = vi.fn();
    render(<CareerHeatmap activities={buildPlayActivity(fillCareerDays("2026-10-01", "2026-10-31", careerFixtureDays), careerFixtureToday)} period={{ view: "month", year: 2026, month: 9 }} selected="2026-10-07" onSelect={select} />);
    const day = screen.getByRole("button", { name: /2026-10-07/ }); day.focus();
    fireEvent.keyDown(day, { key: "ArrowRight" });
    expect(screen.getByRole("button", { name: /2026-10-08/ })).toHaveFocus();
    await user.keyboard("{Enter}"); expect(select).toHaveBeenCalledWith("2026-10-08");
    fireEvent.keyDown(document.activeElement!, { key: "End" });
    expect(screen.getByRole("button", { name: /2026-10-08/ })).toHaveFocus();
    expect(screen.getByRole("button", { name: /2026-10-09/ })).toBeDisabled();
  });
});
