import { QueryClient } from "@tanstack/react-query";
import { describe, expect, it } from "vitest";
import type { LocalIndexLoadStatus, OsuClient } from "../../shared/types/osu";
import { invalidateLocalClient, observeLocalIndex } from "./indexCache";

function status(stable: string | null, lazer: string | null): LocalIndexLoadStatus {
  const client = (last_scan_at: string | null) => ({
    last_scan_at, phase: "watching", pending_changes: 0, last_change_at: null,
    added: 0, modified: 0, removed: 0, reused: 0,
  });
  return { phase: "ready", error: null, clients: { stable: client(stable), lazer: client(lazer) } };
}

function cacheLibrary(queryClient: QueryClient, client: OsuClient) {
  const keys = [
    ["local-beatmap-background", client, "cover", "stage"],
    ["local-complete-set", client, "set", "osu"],
    ["local-beatmap-sets", { client, ruleset: "osu" }],
  ];
  keys.forEach((key) => queryClient.setQueryData(key, { cached: true }));
  return keys;
}

describe("local index cache", () => {
  it("refreshes a startup cache miss even if the first observed status is already ready", () => {
    const queries = new QueryClient();
    const key = ["local-summary", "lazer"];
    queries.setQueryData(key, null);
    observeLocalIndex(queries, status(null, null));
    expect(queries.getQueryState(key)?.isInvalidated).toBe(true);
    queries.setQueryData(key, { scanned_at: "saved-revision" });
    observeLocalIndex(queries, status(null, null));
    expect(queries.getQueryState(key)?.isInvalidated).toBe(false);
    queries.clear();
  });
  it("reuses resources across page remounts and mode/client switches", () => {
    const queries = new QueryClient();
    observeLocalIndex(queries, status("revision-1", "revision-2"));
    const keys = [...cacheLibrary(queries, "stable"), ...cacheLibrary(queries, "lazer")];
    observeLocalIndex(queries, status("revision-1", "revision-2"));
    observeLocalIndex(queries, status("revision-1", "revision-2"));
    keys.forEach((key) => expect(queries.getQueryState(key)?.isInvalidated).toBe(false));
    queries.clear();
  });

  it("invalidates only the changed client's resources, including complete sets", async () => {
    const queries = new QueryClient();
    const stable = cacheLibrary(queries, "stable");
    const lazer = cacheLibrary(queries, "lazer");
    await invalidateLocalClient(queries, "stable");
    stable.forEach((key) => expect(queries.getQueryState(key)?.isInvalidated).toBe(true));
    lazer.forEach((key) => expect(queries.getQueryState(key)?.isInvalidated).toBe(false));
    queries.clear();
  });

  it("refreshes results after startup index loading and after a later scan", () => {
    const queries = new QueryClient();
    observeLocalIndex(queries, { phase: "loading", error: null });
    const keys = cacheLibrary(queries, "stable");
    observeLocalIndex(queries, status(null, null));
    keys.forEach((key) => expect(queries.getQueryState(key)?.isInvalidated).toBe(true));
    cacheLibrary(queries, "stable");
    observeLocalIndex(queries, status("new-scan", null));
    keys.forEach((key) => expect(queries.getQueryState(key)?.isInvalidated).toBe(true));
    queries.clear();
  });
});
