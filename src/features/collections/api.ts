import { useQuery, useQueryClient } from "@tanstack/react-query";
import { desktopApi } from "../../shared/lib/tauri";
import type { CollectionSnapshot, CommandError } from "../../shared/types/osu";
import type { CollectionBrowseQuery } from "../../shared/types/osu";

export const collectionBrowserKey = ["collections", "browser"] as const;
export function useCollectionBrowser(query: CollectionBrowseQuery, enabled: boolean) {
  return useQuery({ queryKey: [...collectionBrowserKey, query], queryFn: () => desktopApi.queryCollectionBrowser(query), enabled, staleTime: 15_000 });
}
export function useCollectionArtwork(folderId: string | null, offset = 0, enabled = true) {
  return useQuery({ queryKey: ["collections", "artwork", folderId, offset], queryFn: () => desktopApi.getCollectionArtwork(folderId, offset), enabled, staleTime: Infinity, gcTime: 5 * 60_000 });
}

export const collectionsQueryKey = ["collections"] as const;

export function removeFromCollectionsSnapshot(
  snapshot: CollectionSnapshot | undefined,
  folderId: string,
  entryId?: string,
) {
  if (!snapshot) return snapshot;
  if (!entryId) {
    return { ...snapshot, folders: snapshot.folders.filter((folder) => folder.id !== folderId) };
  }
  return {
    ...snapshot,
    folders: snapshot.folders.map((folder) => folder.id === folderId
      ? {
          ...folder,
          entries: folder.entries.filter((entry) => entry.id !== entryId),
          pending_write: folder.stable_sync !== false,
          updated_at: new Date().toISOString(),
        }
      : folder),
  };
}

export function useCollections(enabled = true) {
  return useQuery({ enabled, queryKey: collectionsQueryKey, queryFn: () => desktopApi.listCollections(), staleTime: 15_000 });
}

export function useRefreshCollections() {
  const queryClient = useQueryClient();
  return async (client: "stable" | "lazer") => {
    const snapshot = await desktopApi.refreshCollectionSummaries(client);
    queryClient.setQueryData(collectionSummariesKey, snapshot);
    await queryClient.invalidateQueries({ queryKey: collectionsQueryKey });
    return snapshot;
  };
}

export function invalidateCollections(queryClient: ReturnType<typeof useQueryClient>) {
  return queryClient.invalidateQueries({ queryKey: collectionsQueryKey });
}

export const collectionSummariesKey = ["collections", "summaries"] as const;
export const collectionEntriesKey = (folderId: string) => ["collection-entries", folderId] as const;
export function useCollectionSummaries(enabled = true) {
  return useQuery({ enabled, queryKey: collectionSummariesKey, queryFn: desktopApi.listCollectionSummaries, staleTime: 15_000 });
}
export function useCollectionEntries(folderId: string, offset: number, revision: number) {
  return useQuery({ queryKey: [...collectionEntriesKey(folderId), revision, offset],
    queryFn: () => desktopApi.queryCollectionEntries(folderId, offset, 12, revision), staleTime: Infinity, gcTime: 60_000,
    retry: (count, error) => (error as unknown as CommandError).code !== "COLLECTION_REVISION_CHANGED" && count < 3 });
}
