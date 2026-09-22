import { useQuery, useQueryClient } from "@tanstack/react-query";
import { desktopApi } from "../../shared/lib/tauri";
import type { BeatmapHubPack, OnlineBeatmapset } from "../../shared/types/osu";

export const hubKey = ["beatmaphub"] as const;
export const beatmapHubAuthKey = [...hubKey, "auth"] as const;
export const beatmapHubProfileKey = [...hubKey, "profile"] as const;
export const beatmapHubRecommendationsKey = [...hubKey, "recommendations"] as const;
export const hubPackKey = [...hubKey, "pack"] as const;
export const hubSearchKey = [...hubKey, "search"] as const;

// All Hub IO crosses this boundary; components never call the command adapter.
export const hubApi = {
  createProfile: desktopApi.createBeatmapHubProfile,
  linkDevice: desktopApi.linkBeatmapHubDevice,
  login: desktopApi.loginBeatmapHub,
  logout: desktopApi.logoutBeatmapHub,
  createDeviceLink: desktopApi.createBeatmapHubDeviceLink,
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
  beginTask: desktopApi.beginCollectionTask,
  downloadItems: desktopApi.getCollectionDownloadItems,
  download: desktopApi.downloadOnlineBeatmapsets,
  install: desktopApi.installCollectionDownloads,
  onDownloadProgress: desktopApi.onBeatmapDownloadProgress,
  refreshRecommendations: () => desktopApi.getBeatmapHubRecommendations(20, true),
};

export function useBeatmapHubAuth() {
  return useQuery({ queryKey: beatmapHubAuthKey, queryFn: desktopApi.getBeatmapHubAuthStatus, retry: false, staleTime: 15_000 });
}

export function useBeatmapHubProfile(enabled: boolean) {
  return useQuery({ queryKey: beatmapHubProfileKey, queryFn: desktopApi.getBeatmapHubProfile, enabled, retry: false, staleTime: 15_000 });
}

export function useBeatmapHubRecommendations() {
  return useQuery({
    queryKey: beatmapHubRecommendationsKey,
    queryFn: () => desktopApi.getBeatmapHubRecommendations(20),
    retry: false,
    staleTime: 5 * 60_000,
  });
}

export function useHubSearch(query: string) {
  return useQuery({ queryKey: [...hubSearchKey, query], queryFn: () => desktopApi.searchBeatmapHubPacks(query), enabled: Boolean(query), retry: false, staleTime: 60_000 });
}
export function useHubPreview(id: string, identity: string) {
  return useQuery({ queryKey: [...hubPackKey, id, identity], queryFn: () => desktopApi.previewBeatmapHubPack(id), enabled: Boolean(id), retry: false, staleTime: 0 });
}
export function useHubComments(id: string) {
  return useQuery({ queryKey: [...hubKey, "comments", id], queryFn: () => desktopApi.getBeatmapHubComments(id), enabled: Boolean(id), retry: false });
}
export function useHubCollections(enabled: boolean) {
  return useQuery({ queryKey: ["collections", "summaries"], queryFn: desktopApi.listCollectionSummaries, enabled, retry: false });
}
export function useHubDownloadConfig(enabled: boolean) {
  return useQuery({ queryKey: [...hubKey, "download-config"], queryFn: async () => {
    const [settings, sources] = await Promise.all([desktopApi.getSettings(), desktopApi.getLocalSources()]);
    const stable = sources.find((source) => source.client === "stable" && source.valid && source.install_root);
    return { settings, root: stable?.install_root ?? null };
  }, enabled, retry: false, staleTime: 0 });
}
export function useHubMetadata(pack: BeatmapHubPack | undefined) {
  const client = useQueryClient();
  return useQuery({
    queryKey: [...hubKey, "metadata", pack?.id, pack?.manifest_hash, pack?.beatmapset_ids],
    enabled: Boolean(pack), retry: false, staleTime: 5 * 60_000,
    queryFn: async ({ signal }) => {
      const ids = pack!.beatmapset_ids;
      const resolved: OnlineBeatmapset[] = [];
      for (let offset = 0; offset < ids.length; offset += 6) {
        if (signal.aborted) throw new Error("解析已取消");
        const batch = await Promise.all(ids.slice(offset, offset + 6).map((id) => client.fetchQuery({
          queryKey: [...hubKey, "beatmapset", id], queryFn: () => desktopApi.getOnlineBeatmapset(id), staleTime: 30 * 60_000, retry: false,
        }).catch(() => null)));
        resolved.push(...batch.filter((set): set is OnlineBeatmapset => set !== null));
      }
      return resolved;
    },
  });
}
