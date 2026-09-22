import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { desktopApi } from "../../shared/lib/tauri";
import type { TournamentPoolRef } from "../../shared/types/osu";
import { collectionEntriesKey, collectionsQueryKey } from "../collections/api";

export const tournamentPoolKey = (reference: TournamentPoolRef) =>
  ["tournament-pool", reference] as const;

export function useTournamentPool(reference: TournamentPoolRef) {
  return useQuery({ queryKey: tournamentPoolKey(reference), queryFn: () => desktopApi.getTournamentPool(reference),
    retry: false, staleTime: 0, refetchOnWindowFocus: false });
}

export function useSyncTournamentPool(reference: TournamentPoolRef) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => desktopApi.syncTournamentPoolCollection(reference),
    onSuccess: async (result) => {
      await queryClient.cancelQueries({ queryKey: tournamentPoolKey(reference) });
      queryClient.setQueryData(tournamentPoolKey(reference), result.pool);
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: collectionsQueryKey }),
        queryClient.invalidateQueries({ queryKey: collectionEntriesKey(result.folder_id) }),
      ]);
    },
  });
}
