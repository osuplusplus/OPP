import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { desktopApi } from "../../shared/lib/tauri";
import { BeatmapSetPanel } from "./BeatmapSetPanel";
import { stageSet } from "./stageFixtures.test-data";
import type { LocalBeatmapSetSummary } from "../../shared/types/osu";
import type { MusicLocation } from "../../shared/types/music";

vi.mock("../../shared/lib/tauri", () => ({ desktopApi: {
  queryLocalBeatmapSets: vi.fn(), pickRandomLocalBeatmapSet: vi.fn(), getLocalBeatmapSet: vi.fn(), getLocalBeatmapBackground: vi.fn(),
  getLocalBeatmapAudio: vi.fn(), openLocalResourceInExplorer: vi.fn(), openExternal: vi.fn(), openNeteaseMusicSearch: vi.fn(),
} }));
vi.mock("../settings/api", () => ({ useSettings: () => ({ data: { preview_volume: 65 } }) }));

const other: LocalBeatmapSetSummary = { ...stageSet, set_key: "set-2", title: "Another Song", difficulties: stageSet.difficulties.map((d) => ({ ...d, set_key: "set-2", resource: { ...d.resource, resource_id: `other-${d.resource.resource_id}` } })) };
function mount(followTarget?: MusicLocation | null) {
  const onOpen = vi.fn();
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: 0 } } });
  const result = render(<QueryClientProvider client={client}><BeatmapSetPanel client="stable" ruleset="osu" onOpen={onOpen} followTarget={followTarget} /></QueryClientProvider>);
  return { ...result, onOpen };
}

describe("single-set local workspace", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.clearAllMocks();
    vi.mocked(desktopApi.getLocalBeatmapBackground).mockResolvedValue(null);
    vi.mocked(desktopApi.pickRandomLocalBeatmapSet).mockResolvedValue(stageSet);
    vi.mocked(desktopApi.getLocalBeatmapSet).mockImplementation(async (_client, key) => key === other.set_key ? other : stageSet);
    vi.mocked(desktopApi.queryLocalBeatmapSets).mockResolvedValue({ items: [stageSet, other], total: 2, offset: 0, limit: 20 });
  });

  it("starts with a random set, keeps all difficulties visible, and exposes actions", async () => {
    const user = userEvent.setup();
    const { onOpen } = mount();
    expect(await screen.findByRole("heading", { name: "Local Song" })).toBeVisible();
    expect(desktopApi.pickRandomLocalBeatmapSet).toHaveBeenCalledWith(expect.objectContaining({ client: "stable", rulesets: ["osu"] }), null);
    expect(screen.getAllByRole("tab")).toHaveLength(2);
    await user.click(screen.getByRole("tab", { name: /Insane/ }));
    await user.click(screen.getByRole("button", { name: "完整详情" }));
    expect(onOpen).toHaveBeenCalledWith("502");
    expect(screen.getByRole("button", { name: "加入收藏夹" })).toBeVisible();
    expect(desktopApi.getLocalBeatmapAudio).not.toHaveBeenCalled();
  });

  it("follows the playing set and difficulty without moving keyboard focus", async () => {
    const first: MusicLocation = { client: "stable", ruleset: "osu", set_key: "set-2", resource_id: "other-502" };
    const second: MusicLocation = { client: "stable", ruleset: "osu", set_key: "set-1", resource_id: "501" };
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: 0 } } });
    const view = (target: MusicLocation) => <QueryClientProvider client={queryClient}><BeatmapSetPanel client="stable" ruleset="osu" onOpen={() => {}} followTarget={target} /></QueryClientProvider>;
    const { rerender } = render(view(first));
    await screen.findByRole("heading", { name: "Another Song" });
    expect(screen.getByRole("tab", { name: /Insane/ })).toHaveAttribute("aria-selected", "true");
    rerender(view(second));
    await screen.findByRole("heading", { name: "Local Song" });
    expect(screen.getByRole("tab", { name: /Easy/ })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tab", { name: /Easy/ })).not.toHaveFocus();
    await userEvent.click(screen.getByRole("combobox"));
    await userEvent.click(await screen.findByRole("option", { name: /Another Song/ }));
    await screen.findByRole("heading", { name: "Another Song" });
    rerender(view(second));
    expect(screen.getByRole("heading", { name: "Another Song" })).toBeVisible();
  });

  it("only changes the stage on confirmation and transfers focus to the matching difficulty", async () => {
    const user = userEvent.setup(); mount();
    await screen.findByRole("heading", { name: "Local Song" });
    const input = screen.getByRole("combobox");
    await user.click(input);
    await screen.findByRole("option", { name: /Another Song/ });
    await user.keyboard("{ArrowDown}");
    expect(screen.getByRole("heading", { name: "Local Song" })).toBeVisible();
    await user.keyboard("{Enter}");
    await screen.findByRole("heading", { name: "Another Song" });
    expect(screen.queryByRole("heading", { name: "Local Song" })).not.toBeInTheDocument();
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /Easy/ })).toHaveFocus();
    await user.keyboard("{ArrowRight}");
    expect(screen.getByRole("tab", { name: /Insane/ })).toHaveAttribute("aria-selected", "true");
  });

  it("expands a filtered match to the full set and selects its matching difficulty", async () => {
    const user = userEvent.setup(); mount();
    await screen.findByRole("heading", { name: "Local Song" });
    vi.mocked(desktopApi.queryLocalBeatmapSets).mockResolvedValue({ items: [{ ...stageSet, difficulties: [stageSet.difficulties[0]] }], total: 1, offset: 0, limit: 20 });
    await user.type(screen.getByRole("combobox"), "Insane");
    await waitFor(() => expect(screen.getByRole("option")).not.toBeDisabled());
    await user.keyboard("{Enter}");
    await waitFor(() => expect(screen.getByRole("tab", { name: /Insane/ })).toHaveAttribute("aria-selected", "true"));
    expect(screen.getAllByRole("tab")).toHaveLength(2);
    expect(screen.getByRole("combobox")).toHaveValue("Insane");
  });

  it("does not commit stale search results or an IME confirmation", async () => {
    const user = userEvent.setup(); mount();
    await screen.findByRole("heading", { name: "Local Song" });
    await user.click(screen.getByRole("combobox"));
    await screen.findByRole("option", { name: /Another Song/ });
    let resolve!: (value: { items: LocalBeatmapSetSummary[]; total: number; offset: number; limit: number }) => void;
    vi.mocked(desktopApi.queryLocalBeatmapSets).mockReturnValue(new Promise((r) => { resolve = r; }));
    await user.type(screen.getByRole("combobox"), "new");
    await user.keyboard("{Enter}");
    expect(desktopApi.getLocalBeatmapSet).toHaveBeenCalledTimes(1);
    await act(async () => resolve({ items: [other], total: 1, offset: 0, limit: 20 }));
    await waitFor(() => expect(screen.getByRole("option")).not.toBeDisabled());
    screen.getByRole("combobox").dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", isComposing: true, bubbles: true }));
    expect(desktopApi.getLocalBeatmapSet).toHaveBeenCalledTimes(1);
  });

  it("ignores an older full-set response after a newer selection", async () => {
    const user = userEvent.setup(); mount();
    await screen.findByRole("heading", { name: "Local Song" });
    let resolve!: (set: LocalBeatmapSetSummary) => void;
    vi.mocked(desktopApi.getLocalBeatmapSet).mockImplementation(async (_client, key) => key === other.set_key ? new Promise((r) => { resolve = r; }) : stageSet);
    await user.click(screen.getByRole("combobox"));
    await user.click(await screen.findByRole("option", { name: /Another Song/ }));
    await user.click(screen.getByRole("combobox"));
    await user.click(await screen.findByRole("option", { name: /Local Song/ }));
    await waitFor(() => expect(screen.getByRole("region", { name: "本地谱面工作区" })).toHaveAttribute("aria-busy", "false"));
    await act(async () => resolve(other));
    expect(screen.getByRole("heading", { name: "Local Song" })).toBeVisible();
  });

  it("preserves the selected set when random has no alternative and honors query filters", async () => {
    const user = userEvent.setup(); mount();
    await screen.findByRole("heading", { name: "Local Song" });
    await user.type(screen.getByRole("combobox"), "Local");
    await user.keyboard("{Escape}");
    await user.click(screen.getByRole("button", { name: "随机一首" }));
    await screen.findByText("当前条件下只有这一个谱面集。");
    expect(desktopApi.pickRandomLocalBeatmapSet).toHaveBeenLastCalledWith(expect.objectContaining({ search: "Local" }), "set-1");
    expect(screen.getByRole("heading", { name: "Local Song" })).toBeVisible();
  });

  it("refreshes the current set after indexing without drawing another random set", async () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: 0 } } });
    const view = (revision: string) => <QueryClientProvider client={client}><BeatmapSetPanel client="stable" ruleset="osu" onOpen={() => {}} libraryRevision={revision} /></QueryClientProvider>;
    const { rerender } = render(view("first"));
    await screen.findByRole("heading", { name: "Local Song" });
    vi.mocked(desktopApi.getLocalBeatmapSet).mockResolvedValue({ ...stageSet, title: "Updated Song" });
    rerender(view("second"));
    await screen.findByRole("heading", { name: "Updated Song" });
    expect(desktopApi.pickRandomLocalBeatmapSet).toHaveBeenCalledTimes(1);
    vi.mocked(desktopApi.getLocalBeatmapSet).mockRejectedValue({ code: "LOCAL_RESOURCE_NOT_FOUND", message: "谱面已移除，请重新选谱" });
    rerender(view("third"));
    await screen.findByText("谱面已移除，请重新选谱");
    expect(screen.queryByRole("button", { name: "试听本地音频" })).not.toBeInTheDocument();
  });

  it("restores selection and filters after the full window is recreated", async () => {
    const user = userEvent.setup();
    const first = mount();
    await screen.findByRole("heading", { name: "Local Song" });
    await user.click(screen.getByRole("tab", { name: /Insane/ }));
    await user.type(screen.getByRole("combobox"), "saved search");
    first.unmount(); vi.mocked(desktopApi.pickRandomLocalBeatmapSet).mockClear();
    mount();
    await screen.findByRole("heading", { name: "Local Song" });
    expect(screen.getByRole("combobox")).toHaveValue("saved search");
    expect(screen.getByRole("tab", { name: /Insane/ })).toHaveAttribute("aria-selected", "true");
    expect(desktopApi.pickRandomLocalBeatmapSet).not.toHaveBeenCalled();
  });
});
