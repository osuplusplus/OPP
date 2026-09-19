import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MusicPlayer } from "./MusicPlayer";
import { acceptMusicState } from "./api";
import { emptyMusicState, musicDesktop } from "../../shared/lib/musicTauri";
import type { MusicState } from "../../shared/types/music";
import { MusicQueue } from "./MusicQueue";

vi.mock("../../shared/lib/musicTauri", async (original) => {
  const real = await original<typeof import("../../shared/lib/musicTauri")>();
  return { ...real, musicDesktop: { available: () => true, state: vi.fn(), subscribe: vi.fn(async () => () => {}), onWindowError: vi.fn(async () => () => {}), artwork: vi.fn(async () => null), queue: vi.fn(), control: vi.fn(async () => {}), setQueue: vi.fn(async () => {}), mode: vi.fn(async () => {}), ready: vi.fn(async () => ({ route: "", pinned: true })), layout: vi.fn(async () => {}), drag: vi.fn(async () => {}), collections: vi.fn(async () => ({ folders: [{ id: "favorite", name: "喜欢" }], sources: [] })), hide: vi.fn(), exit: vi.fn() } };
});
let version = 0;
function snapshot(overrides: Partial<MusicState> = {}): MusicState { return { ...emptyMusicState, ...overrides, version: ++version }; }
beforeEach(() => {
  vi.clearAllMocks();
  const state = snapshot(); acceptMusicState(state);
  vi.mocked(musicDesktop.state).mockResolvedValue(state);
  vi.mocked(musicDesktop.queue).mockResolvedValue({ items: [], total: 0, version: 0 });
});
afterEach(cleanup);
describe("local player panel", () => {
  it("keeps a large queue scrollable while fetching the next page", async () => {
    const state = snapshot({ total: 1000, queue_version: 8 });
    vi.mocked(musicDesktop.queue).mockResolvedValueOnce({ items: [], total: 1000, version: 8 });
    const { container } = render(<MusicQueue state={state} onError={vi.fn()} />);
    await waitFor(() => expect(musicDesktop.queue).toHaveBeenCalledWith(0,62,""));
    vi.mocked(musicDesktop.queue).mockImplementation(() => new Promise(() => {}));
    const scroll = container.querySelector(".music-queue-scroll") as HTMLDivElement;
    fireEvent.scroll(scroll, { target: { scrollTop: 50 * 48 } });
    await waitFor(() => expect(musicDesktop.queue).toHaveBeenCalledWith(50,62,""));
    expect(scroll.firstElementChild).toHaveStyle({ height: "48000px" });
  });
  it("adds the selected map and plays a collection using resource references", async () => {
    render(<MusicPlayer client="lazer" resourceId="map" />);
    await userEvent.click(screen.getByRole("button", { name: "展开播放列表" }));
    await screen.findByRole("option", { name: "收藏夹 · 喜欢" });
    await userEvent.click(screen.getByRole("button", { name: "将当前谱面加入播放列表" }));
    expect(musicDesktop.setQueue).toHaveBeenCalledWith({ client: "lazer", resource_id: "map", append: true });
    fireEvent.change(screen.getByLabelText("歌曲来源"), { target: { value: "collection:favorite" } });
    await userEvent.click(screen.getByRole("button", { name: "播放所选来源" }));
    expect(musicDesktop.setQueue).toHaveBeenCalledWith({ collection_id: "favorite" });
  });
  it("ignores stale events and keeps playing when the viewed map changes", async () => {
    const current = snapshot({ playing: true, total: 1, current: { id: "online:1", title: "正在播放", artist: "曲师", clients: ["stable","lazer"] } });
    acceptMusicState(current); vi.mocked(musicDesktop.state).mockResolvedValue(current);
    const { rerender } = render(<MusicPlayer client="stable" resourceId="a" />);
    await screen.findByText("正在播放");
    act(() => acceptMusicState({ ...current, version: current.version - 1, playing: false }));
    rerender(<MusicPlayer client="lazer" resourceId="b" />);
    expect(screen.getByRole("button", { name: "暂停音乐" })).toBeEnabled();
    expect(musicDesktop.control).not.toHaveBeenCalled(); expect(musicDesktop.setQueue).not.toHaveBeenCalled();
  });
  it("controls mini expansion, pinning and restoration without restarting playback", async () => {
    render(<MusicPlayer mini />);
    await waitFor(() => expect(musicDesktop.ready).toHaveBeenCalled());
    await userEvent.click(screen.getByRole("button", { name: "展开播放列表" }));
    expect(musicDesktop.layout).toHaveBeenCalledWith(true,true);
    await userEvent.click(screen.getByRole("button", { name: "取消置顶" }));
    expect(musicDesktop.layout).toHaveBeenCalledWith(true,false);
    await userEvent.click(screen.getByRole("button", { name: "恢复完整界面" }));
    expect(musicDesktop.mode).toHaveBeenCalledWith(false); expect(musicDesktop.setQueue).not.toHaveBeenCalled();
  });
  it("drags the mini window from its artwork or text without stealing control clicks", async () => {
    const { container } = render(<MusicPlayer mini />);
    fireEvent.mouseDown(container.querySelector(".music-cover")!, { button: 0 });
    fireEvent.mouseDown(screen.getByText("本地音乐"), { button: 0 });
    expect(musicDesktop.drag).toHaveBeenCalledTimes(2);
    fireEvent.mouseDown(screen.getByRole("button", { name: "下一首" }), { button: 0 });
    fireEvent.mouseDown(screen.getByRole("button", { name: "展开播放列表" }), { button: 0 });
    fireEvent.mouseDown(container.querySelector(".music-progress")!, { button: 0 });
    expect(musicDesktop.drag).toHaveBeenCalledTimes(2);
  });
  it("sends playback mode and volume controls and shows failures", async () => {
    const playing = snapshot({ duration: 180, total: 1, current: { id: "online:1", title: "测试歌曲", artist: "曲师", clients: ["stable"] } });
    acceptMusicState(playing); vi.mocked(musicDesktop.state).mockResolvedValue(playing);
    vi.mocked(musicDesktop.queue).mockResolvedValue({ items: [playing.current!], total: 1, version: 0 });
    render(<MusicPlayer />); await userEvent.click(screen.getByRole("button", { name: "展开播放列表" }));
    fireEvent.change(screen.getByLabelText("音乐播放进度"), { target: { value: "60" } });
    fireEvent.pointerUp(screen.getByLabelText("音乐播放进度"));
    await waitFor(() => expect(musicDesktop.control).toHaveBeenCalledWith({ action: "seek", seconds: 60 }));
    await userEvent.click(await screen.findByRole("button", { name: "移除 测试歌曲" }));
    expect(musicDesktop.control).toHaveBeenCalledWith({ action: "remove", id: "online:1" });
    await userEvent.click(screen.getByRole("button", { name: "清空播放列表" }));
    expect(musicDesktop.control).toHaveBeenCalledWith({ action: "clear" });
    fireEvent.change(screen.getByLabelText("播放方式"), { target: { value: "shuffle" } });
    await waitFor(() => expect(musicDesktop.control).toHaveBeenCalledWith({ action: "mode", mode: "shuffle" }));
    fireEvent.change(screen.getByLabelText("音乐音量"), { target: { value: "0.25" } });
    await waitFor(() => expect(musicDesktop.control).toHaveBeenCalledWith({ action: "volume", value: .25 }));
    vi.mocked(musicDesktop.mode).mockRejectedValueOnce(new Error("窗口创建失败"));
    await userEvent.click(screen.getByRole("button", { name: "切换迷你播放器" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("窗口创建失败");
  });
});
