import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { desktopApi } from "../lib/tauri";
import { ArtworkMosaic } from "./ArtworkMosaic";

afterEach(() => vi.restoreAllMocks());

it("loads at most 24 distinct thumbnails, reuses them on remount and pauses when hidden", async () => {
  const hidden = vi.spyOn(document, "hidden", "get").mockReturnValue(false);
  const background = vi.spyOn(desktopApi, "getLocalBeatmapBackground").mockImplementation(async (_client, id) => `image-${id}`);
  const queries = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const candidates = Array.from({ length: 80 }, (_, i) => ({ client: "stable" as const, resource_id: String(Math.floor(i / 2)) }));
  const element = <QueryClientProvider client={queries}><ArtworkMosaic candidates={candidates} /></QueryClientProvider>;
  const first = render(element);
  await waitFor(() => expect(first.container.querySelectorAll("img")).toHaveLength(24));
  expect(background).toHaveBeenCalledTimes(24);
  expect(background).toHaveBeenCalledWith("stable", "23", "thumbnail");
  expect(first.container.querySelector(".is-moving")).not.toBeNull();
  hidden.mockReturnValue(true);
  act(() => document.dispatchEvent(new Event("visibilitychange")));
  expect(first.container.querySelector(".is-moving")).toBeNull();
  first.unmount();
  hidden.mockReturnValue(false);
  const second = render(element);
  await waitFor(() => expect(second.container.querySelectorAll("img")).toHaveLength(24));
  expect(background).toHaveBeenCalledTimes(24);
  second.unmount();
  queries.clear();
});
