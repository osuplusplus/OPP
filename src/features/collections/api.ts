import { useQuery, useQueryClient } from "@tanstack/react-query";
import { desktopApi } from "../../shared/lib/tauri";
import type { CollectionBrowseQuery } from "../../shared/types/osu";

export const collectionBrowserKey = ["collections", "browser"] as const;
export function useCollectionBrowser(query: CollectionBrowseQuery, enabled: boolean) {
  return useQuery({ queryKey: [...collectionBrowserKey, query], queryFn: () => desktopApi.queryCollectionBrowser(query), enabled, staleTime: 15_000 });
}
export function useCollectionArtwork(folderId: string | null, offset = 0, enabled = true) {
  return useQuery({ queryKey: ["collections", "artwork", folderId, offset], queryFn: () => desktopApi.getCollectionArtwork(folderId, offset), enabled, staleTime: Infinity, gcTime: 5 * 60_000 });
}

export const collectionsQueryKey = ["collections"] as const;

export function useRefreshCollections() {
  const queryClient = useQueryClient();
  return async (client: "stable" | "lazer") => {
    const snapshot = await desktopApi.refreshCollectionSummaries(client);
    queryClient.setQueryData(collectionSummariesKey, snapshot);
    await queryClient.invalidateQueries({ queryKey: collectionsQueryKey });
    return snapshot;
  };
}

export const collectionSummariesKey = ["collections", "summaries"] as const;
export const collectionEntriesKey = (folderId: string) => ["collection-entries", folderId] as const;
export function useCollectionSummaries(enabled = true) {
  return useQuery({ enabled, queryKey: collectionSummariesKey, queryFn: desktopApi.listCollectionSummaries, staleTime: 15_000 });
}
