import { StrictMode } from "react";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { desktopApi } from "../../shared/lib/tauri";
import type { TournamentLink, TournamentPoolRef } from "../../shared/types/osu";
import { TournamentPoolHost } from "./TournamentPoolHost";

const subscriptions = vi.hoisted(() => new Set<(link: TournamentLink) => void>());
vi.mock("./TournamentPoolDialog", () => ({ default: ({ reference, onClose }: { reference: TournamentPoolRef; onClose: () => void }) =>
  <div role="dialog">{reference.season}<button onClick={onClose}>关闭</button></div>,
}));
vi.mock("../../shared/lib/tauri", async (original) => {
  const real = await original<typeof import("../../shared/lib/tauri")>();
  return { ...real, desktopApi: { ...real.desktopApi,
    getPendingTournamentLink: vi.fn(), acknowledgeTournamentLink: vi.fn().mockResolvedValue(undefined),
    onTournamentPoolOpen: vi.fn(async (listener: (link: TournamentLink) => void) => {
      subscriptions.add(listener); return () => { subscriptions.delete(listener); };
    }),
  } };
});
const first: TournamentLink = { id: 1, reference: { provider: "rino", season: "s1", category: "qualification" } };
beforeEach(() => { vi.clearAllMocks(); subscriptions.clear(); vi.mocked(desktopApi.getPendingTournamentLink).mockResolvedValue(first); });
afterEach(cleanup);

it("drains the startup/login inbox once and permits a later click to reopen the same pool", async () => {
  const view = render(<StrictMode><TournamentPoolHost /></StrictMode>);
  expect(await screen.findByRole("dialog")).toHaveTextContent("s1");
  await waitFor(() => expect(subscriptions.size).toBe(1));
  expect(desktopApi.acknowledgeTournamentLink).toHaveBeenCalledExactlyOnceWith(1);
  act(() => subscriptions.forEach((listener) => listener(first)));
  expect(screen.getAllByRole("dialog")).toHaveLength(1);
  await userEvent.click(screen.getByRole("button", { name: "关闭" }));
  act(() => subscriptions.forEach((listener) => listener(first)));
  expect(screen.queryByRole("dialog")).toBeNull();
  act(() => subscriptions.forEach((listener) => listener({ ...first, id: 2 })));
  expect(await screen.findByRole("dialog")).toHaveTextContent("s1");
  view.unmount();
  expect(subscriptions.size).toBe(0);
});

it("does not let a stale startup response replace a newer running-instance link", async () => {
  let resolve!: (link: TournamentLink) => void;
  vi.mocked(desktopApi.getPendingTournamentLink).mockReturnValue(new Promise((done) => { resolve = done; }));
  render(<TournamentPoolHost />);
  await waitFor(() => expect(desktopApi.getPendingTournamentLink).toHaveBeenCalledOnce());
  act(() => subscriptions.forEach((listener) => listener({ id: 2, reference: { ...first.reference, season: "s2" } })));
  expect(await screen.findByRole("dialog")).toHaveTextContent("s2");
  await act(async () => resolve(first));
  expect(screen.getByRole("dialog")).toHaveTextContent("s2");
  expect(desktopApi.acknowledgeTournamentLink).toHaveBeenCalledExactlyOnceWith(2);
});
