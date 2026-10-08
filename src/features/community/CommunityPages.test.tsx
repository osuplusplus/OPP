import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ModeProvider } from "../../app/ModeContext";
import type { CommunityLobby, CommunityTournament } from "../../shared/types/osu";

const api = vi.hoisted(() => Object.fromEntries(["getBeatmapHubAuthStatus", "getBeatmapHubProfile", "getAuthStatus", "loginBeatmapHub", "reconnectBeatmapHub", "listCommunityTournaments", "getCommunityTournament", "listCommunityLobbies", "getCommunityLobby", "saveCommunityLobby", "closeCommunityLobby", "deleteCommunityLobby", "openExternal"].map((name) => [name, vi.fn()])));
vi.mock("../../shared/lib/tauri", () => ({ desktopApi: api }));
import { TournamentsPage } from "./TournamentsPage";
import { LobbiesPage } from "./LobbiesPage";

const future = (hours: number) => new Date(Date.now() + hours * 3600_000).toISOString();
let lobby: CommunityLobby;
let tournament: CommunityTournament;
function renderPage(kind: "lobbies" | "tournaments", path?: string) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  return { ...render(<QueryClientProvider client={client}><MemoryRouter initialEntries={[path ?? `/community/${kind}`]}><ModeProvider>{kind === "lobbies" ? <LobbiesPage /> : <TournamentsPage />}</ModeProvider></MemoryRouter></QueryClientProvider>), client };
}
async function fillDraft() {
  await userEvent.click(screen.getByRole("button", { name: "发布约玩" }));
  await userEvent.type(screen.getByLabelText("标题", { exact: true }), "今晚一起打 MP");
  await userEvent.type(screen.getByLabelText("活动描述"), "4–6 星图，欢迎新人");
}
describe("community pages", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    lobby = { id: "00000000-0000-4000-8000-000000000001", owner_user_id: "owner-uuid", osu_user_id: "12345", osu_username: "Player", title: "Tonight MP", description: "一起练图\n欢迎新人", ruleset: "osu", activity_type: "mp", platform: "romai", starts_at: future(1), ends_at: future(3), closed_at: null, status: "upcoming", created_at: future(-1), updated_at: future(-1) };
    tournament = { id: "00000000-0000-4000-8000-000000000101", title: "Autumn Cup", organizer: "OPP", ruleset: "osu", summary: "报名来比赛", description: "比赛规则\n第二段规则", requirements: "欢迎报名", poster_url: "https://example.com/poster.png", registration_url: "https://example.com/signup", registration_starts_at: future(-1), registration_ends_at: future(5), starts_at: future(10), ends_at: future(20), status: "registering", created_at: future(-1), updated_at: future(-1) };
    api.getBeatmapHubAuthStatus.mockResolvedValue({ connected: false, has_identity: false });
    api.getAuthStatus.mockResolvedValue({ connected: true, username: "Player", user_id: 12345 });
    api.getBeatmapHubProfile.mockResolvedValue({ user: { id: "owner-uuid", display_name: "Player" }, current_device_id: "device", devices: [] });
    api.listCommunityLobbies.mockImplementation(() => Promise.resolve({ items: [lobby], next_cursor: null, server_time: new Date().toISOString() }));
    api.getCommunityLobby.mockImplementation(() => Promise.resolve(lobby));
    api.listCommunityTournaments.mockResolvedValue({ items: [tournament], next_cursor: null, server_time: new Date().toISOString() });
    api.getCommunityTournament.mockResolvedValue(tournament);
    api.openExternal.mockResolvedValue(undefined);
    api.saveCommunityLobby.mockImplementation((input) => { lobby = { ...lobby, ...input }; return Promise.resolve(lobby); });
    api.closeCommunityLobby.mockImplementation(() => { lobby = { ...lobby, status: "closed", closed_at: new Date().toISOString() }; return Promise.resolve(lobby); });
  });
  it("browses tournaments without community identity and provides no publish form", async () => {
    renderPage("tournaments");
    const card = await screen.findByRole("button", { name: "查看比赛 Autumn Cup" });
    expect(screen.getByText(/1059437719/)).toBeVisible();
    expect(screen.queryByRole("button", { name: "发布比赛" })).not.toBeInTheDocument();
    expect(api.getBeatmapHubAuthStatus).not.toHaveBeenCalled();
    fireEvent.error(screen.getByAltText("Autumn Cup赛事海报"));
    expect(screen.getByText("COMMUNITY TOURNAMENT")).toBeVisible();
    await userEvent.click(card);
    const dialog = await screen.findByRole("dialog", { name: "Autumn Cup" });
    expect(await within(dialog).findByText(/第二段规则/)).toBeVisible();
    await userEvent.click(within(dialog).getByRole("button", { name: "打开报名链接" }));
    expect(api.openExternal).toHaveBeenCalledWith("https://example.com/signup");
    await userEvent.keyboard("{Escape}"); await waitFor(() => expect(card).toHaveFocus());
  });
  it("browses lobbies as a guest and filters on the server", async () => {
    renderPage("lobbies"); await screen.findByRole("button", { name: "查看约玩 Tonight MP" });
    expect(api.loginBeatmapHub).not.toHaveBeenCalled(); expect(api.getBeatmapHubProfile).not.toHaveBeenCalled();
    await userEvent.selectOptions(screen.getByLabelText("平台", { exact: true }), "romai");
    await waitFor(() => expect(api.listCommunityLobbies).toHaveBeenLastCalledWith(expect.objectContaining({ platform: "romai", status: "active" }), false));
  });
  it("retains the draft through identity connection without auto-publishing", async () => {
    api.loginBeatmapHub.mockImplementation(() => { api.getBeatmapHubAuthStatus.mockResolvedValue({ connected: true, device_id: "device", user_id: "12345" }); return Promise.resolve({ connected: true }); });
    renderPage("lobbies"); await screen.findByRole("button", { name: "查看约玩 Tonight MP" }); await fillDraft();
    await userEvent.click(screen.getByRole("button", { name: "连接社区身份后继续" }));
    await userEvent.click(within(screen.getByRole("dialog", { name: "连接社区身份" })).getByRole("button", { name: "连接身份" }));
    await screen.findByRole("button", { name: "确认发布" });
    expect(screen.getByLabelText("标题", { exact: true })).toHaveValue("今晚一起打 MP"); expect(api.saveCommunityLobby).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "确认发布" }));
    await waitFor(() => expect(api.saveCommunityLobby).toHaveBeenCalledTimes(1));
    expect(api.saveCommunityLobby).toHaveBeenCalledWith(expect.objectContaining({ title: "今晚一起打 MP", description: "4–6 星图，欢迎新人", ruleset: "osu" }), null);
    await screen.findByRole("dialog", { name: "今晚一起打 MP" });
  });
  it("keeps a failed publish draft and rejects invalid end times", async () => {
    api.getBeatmapHubAuthStatus.mockResolvedValue({ connected: true, device_id: "device" });
    api.saveCommunityLobby.mockRejectedValue({ message: "网络暂时不可用" });
    renderPage("lobbies"); await screen.findByRole("button", { name: "查看约玩 Tonight MP" }); await fillDraft();
    await userEvent.click(screen.getByRole("button", { name: "确认发布" }));
    await screen.findByRole("alert"); expect(screen.getByLabelText("标题", { exact: true })).toHaveValue("今晚一起打 MP");
    fireEvent.change(screen.getByLabelText("结束时间"), { target: { value: "2020-01-01T12:00" } });
    await userEvent.click(screen.getByRole("button", { name: "确认发布" }));
    expect(api.saveCommunityLobby).toHaveBeenCalledTimes(1); expect(screen.getByRole("alert")).toHaveTextContent("结束时间必须晚于");
  });
  it("uses the internal community owner rather than the claimed osu ID for controls", async () => {
    api.getBeatmapHubAuthStatus.mockResolvedValue({ connected: true, device_id: "device", user_id: "12345" });
    api.getBeatmapHubProfile.mockResolvedValue({ user: { id: "different-owner", display_name: "Player" } });
    renderPage("lobbies"); await userEvent.click(await screen.findByRole("button", { name: "查看约玩 Tonight MP" }));
    const dialog = await screen.findByRole("dialog", { name: "Tonight MP" });
    expect(within(dialog).queryByRole("button", { name: "编辑" })).not.toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "打开 osu! 主页" })).toBeVisible();
  });
  it("lets owners close activities through explicit confirmation", async () => {
    api.getBeatmapHubAuthStatus.mockResolvedValue({ connected: true, device_id: "device" });
    renderPage("lobbies"); await userEvent.click(await screen.findByRole("button", { name: "查看约玩 Tonight MP" }));
    await userEvent.click(await screen.findByRole("button", { name: "提前结束" })); expect(api.closeCommunityLobby).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "确认结束" })); await waitFor(() => expect(api.closeCommunityLobby).toHaveBeenCalledWith(lobby.id));
    expect(await screen.findByText("活动已提前结束。")).toBeVisible(); expect(screen.queryByRole("button", { name: "编辑" })).not.toBeInTheDocument();
  });
  it("loads additional pages without duplicate cards and retries list failures", async () => {
    const extra = { ...lobby, id: "00000000-0000-4000-8000-000000000002", title: "Another MP" };
    api.listCommunityLobbies.mockResolvedValueOnce({ items: [lobby], next_cursor: "next", server_time: new Date().toISOString() }).mockResolvedValue({ items: [lobby, extra], next_cursor: null, server_time: new Date().toISOString() });
    renderPage("lobbies"); await userEvent.click(await screen.findByRole("button", { name: "加载更多" }));
    await screen.findByRole("button", { name: "查看约玩 Another MP" }); expect(screen.getAllByRole("button", { name: "查看约玩 Tonight MP" })).toHaveLength(1);
    api.listCommunityLobbies.mockRejectedValue({ message: "服务暂时不可用" });
    await userEvent.click(screen.getByRole("button", { name: "刷新约玩" })); await screen.findByRole("alert");
    api.listCommunityLobbies.mockResolvedValue({ items: [lobby], next_cursor: null, server_time: new Date().toISOString() });
    await userEvent.click(screen.getByRole("button", { name: "重新加载" })); await screen.findByRole("button", { name: "查看约玩 Tonight MP" });
  });
});
