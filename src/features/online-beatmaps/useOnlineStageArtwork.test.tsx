import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, expect, it, vi } from "vitest";
import type { PropsWithChildren } from "react";
import type { OnlineBeatmapset } from "../../shared/types/osu";
import { useOnlineStageArtwork } from "./useOnlineStageArtwork";

const getOnlineBeatmapBackground = vi.hoisted(() => vi.fn());
vi.mock("../../shared/lib/tauri", () => ({
  desktopApi: { getOnlineBeatmapBackground },
}));

const set = (id: number): OnlineBeatmapset => ({
  id,
  title: `Song ${id}`,
  artist: "Artist",
  creator: "Mapper",
  status: "ranked",
  covers: { cover: `cover-${id}.jpg` },
});

beforeEach(() => {
  getOnlineBeatmapBackground.mockReset().mockImplementation(async (id: number) => `original-${id}.jpg`);
});

it("requests the selected original background without an artificial delay", async () => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: PropsWithChildren) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  const { result, rerender } = renderHook(({ beatmapset }) => useOnlineStageArtwork(beatmapset), {
    initialProps: { beatmapset: set(1) },
    wrapper,
  });

  expect(result.current.source).toBe("cover-1.jpg");
  await waitFor(() => expect(getOnlineBeatmapBackground).toHaveBeenCalledWith(1));
  await waitFor(() => expect(result.current.source).toBe("original-1.jpg"));
  rerender({ beatmapset: set(2) });
  expect(result.current.source).toBe("cover-2.jpg");
  await waitFor(() => expect(getOnlineBeatmapBackground).toHaveBeenCalledTimes(2));
  expect(getOnlineBeatmapBackground).toHaveBeenCalledWith(2);
  await waitFor(() => expect(result.current.source).toBe("original-2.jpg"));
});
