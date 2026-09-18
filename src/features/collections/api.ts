import { useQuery, useQueryClient } from "@tanstack/react-query";
import { desktopApi } from "../../shared/lib/tauri";
import type { CollectionSnapshot } from "../../shared/types/osu";

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
          pending_write: true,
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
    const snapshot = await desktopApi.refreshCollections(client);
    queryClient.setQueryData(collectionsQueryKey, snapshot);
    return snapshot;
  };
}

export function invalidateCollections(queryClient: ReturnType<typeof useQueryClient>) {
  return queryClient.invalidateQueries({ queryKey: collectionsQueryKey });
}
