import { StrictMode } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, useLocation } from "react-router-dom";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { desktopApi } from "../../shared/lib/tauri";
import type { TournamentLink } from "../../shared/types/osu";
import { TournamentPoolHost } from "./TournamentPoolHost";
import { createPoolImportSession } from "./importSession";

const subscriptions = vi.hoisted(() => new Set<(link: TournamentLink) => void>());
vi.mock("../../shared/lib/tauri", async (original) => {
  const real = await original<typeof import("../../shared/lib/tauri")>();
  return { ...real, desktopApi: { ...real.desktopApi,
    getPendingTournamentLink: vi.fn(), acknowledgeTournamentLink: vi.fn().mockResolvedValue(undefined),
    onTournamentImportProgress: vi.fn().mockResolvedValue(() => undefined),
    openTournamentPool: vi.fn().mockResolvedValue({ folder_id: "saved", existing: false }),
    onTournamentPoolOpen: vi.fn(async (listener: (link: TournamentLink) => void) => {
      subscriptions.add(listener); return () => { subscriptions.delete(listener); };
    }),
  } };
});
const first: TournamentLink = { id: 1, reference: { provider: "rino", season: "s1", category: "qualification" } };
beforeEach(() => { vi.clearAllMocks(); subscriptions.clear(); vi.mocked(desktopApi.getPendingTournamentLink).mockResolvedValue(first); });
afterEach(cleanup);
function Location() { const location = useLocation(); return <p>{location.pathname}{location.search}</p>; }
function setup() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: 0 } } });
  const session = createPoolImportSession(desktopApi);
  const invalidate = vi.spyOn(client, "invalidateQueries");
  const view = render(<StrictMode><QueryClientProvider client={client}><MemoryRouter><TournamentPoolHost session={session} /><Location /></MemoryRouter></QueryClientProvider></StrictMode>);
  return { ...view, client, session, invalidate };
}

it("drains startup/login once, navigates into collections and invalidates both query families", async () => {
  const { unmount, client, invalidate } = setup();
  expect(await screen.findByText("/collections?folder=saved")).toBeInTheDocument();
  expect(desktopApi.openTournamentPool).toHaveBeenCalledExactlyOnceWith(first.reference, 1);
  expect(desktopApi.acknowledgeTournamentLink).toHaveBeenCalledExactlyOnceWith(1);
  expect(invalidate).toHaveBeenCalledWith({ queryKey: ["collections"] });
  expect(invalidate).toHaveBeenCalledWith({ queryKey: ["collection-entries", "saved"] });
  act(() => subscriptions.forEach((listener) => listener(first)));
  expect(desktopApi.openTournamentPool).toHaveBeenCalledOnce();
  act(() => subscriptions.forEach((listener) => listener({ ...first, id: 2 })));
  await waitFor(() => expect(desktopApi.openTournamentPool).toHaveBeenCalledTimes(2));
  expect(await screen.findByText("/collections?folder=saved")).toBeInTheDocument();
  unmount(); expect(subscriptions.size).toBe(0); client.clear();
});

it("ignores a stale startup result after a running-instance URI", async () => {
  let resolve!: (link: TournamentLink) => void;
  vi.mocked(desktopApi.getPendingTournamentLink).mockReturnValue(new Promise((done) => { resolve = done; }));
  const { client, unmount } = setup();
  await waitFor(() => expect(subscriptions.size).toBe(1));
  const newer: TournamentLink = { id: 2, reference: { provider: "opp", url: "https://example.com/pool.json" } };
  act(() => subscriptions.forEach((listener) => listener(newer)));
  await screen.findByText("/collections?folder=saved");
  await act(async () => resolve(first));
  expect(desktopApi.openTournamentPool).toHaveBeenCalledExactlyOnceWith(newer.reference, 2);
  expect(desktopApi.acknowledgeTournamentLink).toHaveBeenCalledExactlyOnceWith(2);
  unmount(); client.clear();
});
