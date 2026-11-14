import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { desktopApi } from "../../shared/lib/tauri";
import type { BeatmapHubPack, OnlineBeatmapset } from "../../shared/types/osu";
import { communityIdentityKey, useCommunityIdentity, useCommunityProfile } from "../../shared/lib/communityIdentity";
import { collectorSearchParams, type CollectorSort } from "./model";

export const hubKey = ["beatmaphub"] as const;
export const beatmapHubAuthKey = communityIdentityKey;
export const beatmapHubRecommendationsKey = [...hubKey, "recommendations"] as const;
export const hubPackKey = [...hubKey, "pack"] as const;
export const hubSearchKey = [...hubKey, "search"] as const;
export const collectorKey = [...hubKey, "collector"] as const;
export const romaiPoolsKey = [...hubKey, "romai-pools"] as const;
export const romaiPoolKey = [...hubKey, "romai-pool"] as const;

// All Hub IO crosses this boundary; components never call the command adapter.
export const hubApi = {
  login: desktopApi.loginBeatmapHub,
  reconnect: desktopApi.reconnectBeatmapHub,
  logout: desktopApi.logoutBeatmapHub,
  revokeDevice: desktopApi.revokeBeatmapHubDevice,
  publish: desktopApi.publishBeatmapHubPack,
  update: desktopApi.updateBeatmapHubPack,
  deletePack: desktopApi.deleteBeatmapHubPack,
  favorite: desktopApi.favoriteBeatmapHubPack,
  like: desktopApi.likeBeatmapHubPack,
  rate: desktopApi.rateBeatmapHubPack,
  createComment: desktopApi.createBeatmapHubComment,
  updateComment: desktopApi.updateBeatmapHubComment,
  deleteComment: desktopApi.deleteBeatmapHubComment,
  importArchive: desktopApi.importCollectionArchive,
  importPack: desktopApi.importBeatmapHubPack,
  importCollector: desktopApi.importOsuCollectorCollection,
  onCollectorProgress: desktopApi.onOsuCollectorImportProgress,
  beginTask: desktopApi.beginCollectionTask,
  downloadItems: desktopApi.getCollectionDownloadItems,
  download: desktopApi.downloadOnlineBeatmapsets,
  install: desktopApi.installCollectionDownloads,
  onDownloadProgress: desktopApi.onBeatmapDownloadProgress,
};

export const useBeatmapHubAuth = useCommunityIdentity;
export const useBeatmapHubProfile = useCommunityProfile;

export function useBeatmapHubRecommendations() {
  return useQuery({
    queryKey: beatmapHubRecommendationsKey,
    queryFn: () => desktopApi.getBeatmapHubRecommendations(20),
    staleTime: 0, gcTime: 0,
  });
}

export function useHubSearch(query: string) {
  return useQuery({ queryKey: [...hubSearchKey, query], queryFn: () => desktopApi.searchBeatmapHubPacks(query), enabled: Boolean(query), gcTime: 0 });
}
export function useHubPreview(id: string, identity: string) {
  return useQuery({ queryKey: [...hubPackKey, id, identity], queryFn: () => desktopApi.previewBeatmapHubPack(id), enabled: Boolean(id), gcTime: 0 });
}
export function useHubComments(id: string) {
  return useQuery({ queryKey: [...hubKey, "comments", id], queryFn: () => desktopApi.getBeatmapHubComments(id), enabled: Boolean(id), gcTime: 0 });
}
// osu!Collector 搜索浏览：关键词 + 排序驱动搜索端点，游标分页“加载更多”由 useInfiniteQuery 驱动。
export function useCollectorCollections(query: { search: string; sort: CollectorSort }) {
  const params = collectorSearchParams(query);
  return useInfiniteQuery({
    queryKey: [...collectorKey, "search", params.search ?? "", params.sortBy],
    queryFn: ({ pageParam }) => desktopApi.getOsuCollectorCollections({ ...params, cursor: pageParam }),
    initialPageParam: null as number | null,
    getNextPageParam: (lastPage) =>
      lastPage.has_more && lastPage.next_cursor != null ? lastPage.next_cursor : undefined,
    gcTime: 0,
  });
}
export function useCollectorDetail(id: number | null) {
  return useQuery({
    queryKey: [...collectorKey, "detail", id],
    queryFn: () => desktopApi.getOsuCollectorCollection(id!),
    enabled: id != null,
    gcTime: 0,
  });
}
// RomAI 图池索引：后端一小时本地缓存 + ETag 条件刷新，这里延长前端 staleTime 避免频繁穿透。
export function useRomaiPools(enabled: boolean) {
  return useQuery({
    queryKey: romaiPoolsKey,
    queryFn: () => desktopApi.getRomaiPoolSummaries(),
    enabled,
    staleTime: 10 * 60_000,
    gcTime: 0,
  });
}
// 单个 RomAI 图池详情（含补全后的谱面元数据），只在打开弹窗时请求。
export function useRomaiPoolDetail(poolId: string | null) {
  return useQuery({
    queryKey: [...romaiPoolKey, poolId],
    queryFn: () => desktopApi.getTournamentPool({ provider: "romai", pool_id: poolId! }),
    enabled: poolId != null,
    gcTime: 0,
  });
}
export function useHubCollections(enabled: boolean) {
  return useQuery({ queryKey: ["collections", "summaries"], queryFn: desktopApi.listCollectionSummaries, enabled });
}
export function useHubDownloadConfig(enabled: boolean) {
  return useQuery({ queryKey: [...hubKey, "download-config"], queryFn: async () => {
    const [settings, sources] = await Promise.all([desktopApi.getSettings(), desktopApi.getLocalSources()]);
    const stable = sources.find((source) => source.client === "stable" && source.valid && source.install_root);
    return { settings, root: stable?.install_root ?? null };
  }, enabled, gcTime: 0 });
}
export function useHubMetadata(pack: BeatmapHubPack | undefined) {
  return useQuery({
    queryKey: [...hubKey, "metadata", pack?.id, pack?.manifest_hash, pack?.beatmapset_ids],
    enabled: Boolean(pack), gcTime: 0,
    queryFn: async ({ signal }) => {
      const ids = pack!.beatmapset_ids;
      const resolved: OnlineBeatmapset[] = [];
      for (let offset = 0; offset < ids.length; offset += 6) {
        if (signal.aborted) throw new Error("解析已取消");
        const batch = await Promise.all(ids.slice(offset, offset + 6).map((id) => desktopApi.getOnlineBeatmapset(id).catch(() => null)));
        resolved.push(...batch.filter((set): set is OnlineBeatmapset => set !== null));
      }
      return resolved;
    },
  });
}
