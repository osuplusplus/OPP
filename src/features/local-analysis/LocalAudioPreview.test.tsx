import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { desktopApi } from "../../shared/lib/tauri";
import { LocalAudioPreview } from "./LocalAudioPreview";
import type { LocalBeatmapAudioPayload } from "../../shared/types/osu";

vi.mock("../../shared/lib/tauri", () => ({ desktopApi: { getLocalBeatmapAudio: vi.fn() } }));

class FakeAudio {
  static instances: FakeAudio[] = [];
  constructor() { FakeAudio.instances.push(this); }
  paused = true; duration = 120; currentTime = 0; volume = 1; src = "";
  onloadedmetadata: (() => void) | null = null;
  ontimeupdate: (() => void) | null = null;
  onplaying: (() => void) | null = null;
  onpause: (() => void) | null = null;
  onended: (() => void) | null = null;
  onerror: (() => void) | null = null;
  play = vi.fn(async () => { if (this.paused && this.currentTime === 0) this.onloadedmetadata?.(); this.paused = false; this.onplaying?.(); });
  pause = vi.fn(() => { this.paused = true; this.onpause?.(); });
  removeAttribute = vi.fn(); load = vi.fn();
}

describe("local audio preview", () => {
  beforeEach(() => {
    FakeAudio.instances = [];
    vi.stubGlobal("Audio", FakeAudio);
    vi.stubGlobal("URL", Object.assign(URL, { createObjectURL: vi.fn(() => "blob:preview"), revokeObjectURL: vi.fn() }));
    vi.mocked(desktopApi.getLocalBeatmapAudio).mockReset().mockResolvedValue({ bytes_base64: "SUQz", mime_type: "audio/mpeg", preview_time_ms: 45000 });
  });
  afterEach(() => vi.unstubAllGlobals());

  it("loads only on demand, starts at preview time, pauses/resumes and cleans up on leave", async () => {
    const user = userEvent.setup();
    const { unmount } = render(<LocalAudioPreview client="lazer" resourceId="map" volume={35} />);
    expect(desktopApi.getLocalBeatmapAudio).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "试听本地音频" }));
    await screen.findByRole("button", { name: "暂停试听" });
    const audio = FakeAudio.instances[0];
    expect(audio.currentTime).toBe(45); expect(audio.volume).toBe(.35);
    await user.click(screen.getByRole("button", { name: "暂停试听" }));
    expect(audio.paused).toBe(true);
    await user.click(screen.getByRole("button", { name: "试听本地音频" }));
    expect(desktopApi.getLocalBeatmapAudio).toHaveBeenCalledTimes(1);
    unmount(); expect(audio.pause).toHaveBeenCalled(); expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:preview");
  });

  it("discards an audio request resolved after leaving the selected difficulty", async () => {
    const user = userEvent.setup();
    let resolve!: (payload: LocalBeatmapAudioPayload) => void;
    vi.mocked(desktopApi.getLocalBeatmapAudio).mockReturnValue(new Promise((r) => { resolve = r; }));
    const { unmount } = render(<LocalAudioPreview client="stable" resourceId="old" volume={65} />);
    await user.click(screen.getByRole("button", { name: "试听本地音频" }));
    unmount();
    await act(async () => resolve({ bytes_base64: "SUQz", mime_type: "audio/mpeg", preview_time_ms: 0 }));
    expect(FakeAudio.instances).toHaveLength(0);
    expect(URL.createObjectURL).not.toHaveBeenCalled();
  });

  it("falls back from an out-of-range preview time and reports missing audio", async () => {
    const user = userEvent.setup();
    vi.mocked(desktopApi.getLocalBeatmapAudio).mockResolvedValue({ bytes_base64: "SUQz", mime_type: "audio/mpeg", preview_time_ms: 999999 });
    const { unmount } = render(<LocalAudioPreview client="stable" resourceId="map" volume={65} />);
    await user.click(screen.getByRole("button", { name: "试听本地音频" }));
    await waitFor(() => expect(FakeAudio.instances[0]?.currentTime).toBe(0));
    unmount();
    vi.mocked(desktopApi.getLocalBeatmapAudio).mockRejectedValue({ code: "LOCAL_AUDIO_NOT_FOUND", message: "谱面音频缺失" });
    render(<LocalAudioPreview client="stable" resourceId="missing" volume={65} />);
    await user.click(screen.getByRole("button", { name: "试听本地音频" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("谱面音频缺失");
  });
});
