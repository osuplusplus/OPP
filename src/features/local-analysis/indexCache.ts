import type { QueryClient } from "@tanstack/react-query";
import type { LocalIndexLoadStatus, OsuClient } from "../../shared/types/osu";

const observed = new WeakMap<QueryClient, LocalIndexLoadStatus>();
const resourceKeys = new Set([
  "local-beatmaps", "local-beatmap-sets", "local-complete-set", "local-skins",
  "local-beatmap-background", "local-beatmap-detail", "local-skin-detail",
  "local-skin-preview", "local-skin-asset", "local-beatmap-audio",
  "collection-background-image", "local-mosaic-image",
]);

export async function invalidateLocalClient(queryClient: QueryClient, client: OsuClient) {
  await queryClient.invalidateQueries({
    predicate: ({ queryKey, state }) => {
      if (queryKey[0] === "local-artwork-sample") return !Array.isArray(state.data) || state.data.length === 0;
      if (queryKey[0] === "local-sources") return true;
      if (queryKey[0] === "local-beatmap-presence") return queryKey[1] === null || queryKey[1] === client;
      if (queryKey[0] === "local-library-storage") return true;
      if (queryKey[0] === "collections" && ["browser", "artwork"].includes(String(queryKey[1]))) return true;
      if (queryKey[0] !== "local-summary" && !resourceKeys.has(String(queryKey[0]))) return false;
      const scope = queryKey[1];
      return scope === client || (typeof scope === "object" && scope !== null && "client" in scope && scope.client === client);
    },
    // Inactive queries become stale; they must not all start IPC requests at once.
    refetchType: "active",
  });
}

/** Remounting a page is not an index update. Compare actual revisions across mounts. */
export function observeLocalIndex(queryClient: QueryClient, status: LocalIndexLoadStatus) {
  const previous = observed.get(queryClient);
  observed.set(queryClient, status);
  if (status.phase !== "ready") return;
  for (const client of ["stable", "lazer"] as const) {
    const revision = status.clients?.[client]?.last_scan_at;
    if ((!previous || previous.phase !== "ready") ||
        (revision && revision !== previous?.clients?.[client]?.last_scan_at)) {
      void invalidateLocalClient(queryClient, client);
    }
  }
}
