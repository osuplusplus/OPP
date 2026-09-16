import { render, screen, waitFor, cleanup } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { LocalAudioPreview } from "./LocalAudioPreview";
import { emptyMusicState } from "../../shared/lib/musicTauri";

const mocks = vi.hoisted(() => ({ setQueue: vi.fn(), control: vi.fn(), state: {} }));
vi.mock("../music-player/api", async (original) => ({ ...await original<typeof import("../music-player/api")>(), musicApi: mocks, useMusicState: () => mocks.state }));
describe("local audio preview through shared player", () => {
  beforeEach(() => { mocks.state = { ...emptyMusicState }; mocks.setQueue.mockReset().mockResolvedValue(undefined); mocks.control.mockReset().mockResolvedValue(undefined); });
  afterEach(cleanup);
  it("loads only on demand and requests the selected preview point", async () => {
    const { unmount } = render(<LocalAudioPreview client="lazer" resourceId="map" volume={35} />);
    expect(mocks.setQueue).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "试听本地音频" }));
    expect(mocks.setQueue).toHaveBeenCalledWith({ client: "lazer", resource_id: "map", append: true, preview: true });
    unmount(); expect(mocks.control).not.toHaveBeenCalled();
  });
  it("pauses the same shared track and does not restart audio when browsing another difficulty", async () => {
    mocks.state = { ...emptyMusicState, resource_id: "map", playing: true, duration: 120, position: 45 };
    const { rerender } = render(<LocalAudioPreview client="stable" resourceId="map" volume={65} />);
    await userEvent.click(screen.getByRole("button", { name: "暂停试听" }));
    expect(mocks.control).toHaveBeenCalledWith({ action: "toggle" });
    rerender(<LocalAudioPreview client="stable" resourceId="other" volume={65} />);
    expect(screen.getByRole("button", { name: "试听本地音频" })).toBeEnabled();
    expect(mocks.setQueue).not.toHaveBeenCalled();
  });
  it("reports a missing resource without creating a second audio element", async () => {
    const audio = vi.spyOn(window,"Audio"); mocks.setQueue.mockRejectedValue({ message: "谱面音频缺失" });
    render(<LocalAudioPreview client="stable" resourceId="missing" volume={65} />);
    await userEvent.click(screen.getByRole("button", { name: "试听本地音频" }));
    await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("谱面音频缺失"));
    expect(audio).not.toHaveBeenCalled(); audio.mockRestore();
  });
});
