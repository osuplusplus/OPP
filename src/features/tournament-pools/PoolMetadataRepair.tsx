import { useQuery, useQueryClient } from "@tanstack/react-query";
import { desktopApi } from "../../shared/lib/tauri";
import type { CollectionPoolSnapshot } from "../../shared/types/osu";

/** Upgrade old saved pools once per session; fetching metadata never synchronises pool membership. */
export function PoolMetadataRepair({ folderId, pool }: { folderId: string; pool: CollectionPoolSnapshot }) {
  const client = useQueryClient();
  const missing = pool.slots.some((slot) => !slot.metadata);
  const query = useQuery({
    queryKey: ["pool-metadata-repair", folderId], enabled: missing,
    staleTime: Infinity, retry: false, refetchOnMount: false, refetchOnWindowFocus: false,
    queryFn: async () => {
      const result = await desktopApi.repairTournamentPoolMetadata(folderId);
      if (result[0]) await Promise.all([
        client.invalidateQueries({ queryKey: ["collections"] }),
        client.invalidateQueries({ queryKey: ["collection-entries", folderId] }),
      ]);
      return result;
    },
  });
  if (!missing) return null;
  if (query.isFetching) return <small role="status">正在补全谱面资料…</small>;
  if (query.isError || query.data?.[1]) return <button className="collection-metadata-retry" onClick={() => void query.refetch()}>部分谱面资料暂未获取，点击重试</button>;
  return null;
}
