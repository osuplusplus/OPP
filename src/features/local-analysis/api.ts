import { useEffect } from "react";
import { observeLocalIndex } from "./indexCache";
import { keepPreviousData, useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { desktopApi } from "../../shared/lib/tauri";
import type {
  BeatmapQuery,
  OsuClient,
  Ruleset,
  SkinQuery,
} from "../../shared/types/osu";

export function fetchCompleteLocalSet(queryClient: QueryClient, client: OsuClient, setKey: string, ruleset: Ruleset) {
  return queryClient.fetchQuery({ queryKey: ["local-complete-set", client, setKey, ruleset], queryFn: () => desktopApi.getLocalBeatmapSet(client, setKey, ruleset), staleTime: Infinity });
}

export function pickLocalSet(query: BeatmapQuery, excludeSetKey: string | null = null) {
  return desktopApi.pickRandomLocalBeatmapSet(query, excludeSetKey);
}

export const localSourcesKey = ["local-sources"] as const;
export const localIndexStatusKey = ["local-index-status"] as const;
export const localArtworkSampleKey = ["local-artwork-sample"] as const;

/** One small sample per application session; image data is loaded only when displayed. */
export function useLocalArtworkSample() {
  const index = useLocalIndexStatus();
  return useQuery({ queryKey: localArtworkSampleKey, queryFn: desktopApi.getLocalArtworkSample,
    enabled: index.data?.phase === "ready", staleTime: Infinity, gcTime: Infinity,
    refetchOnWindowFocus: false, refetchOnReconnect: false, retry: false });
}
export const localSummaryKey = (client: OsuClient) =>
  ["local-summary", client] as const;
export const localBeatmapsKey = (query: BeatmapQuery) =>
  ["local-beatmaps", query] as const;
export const localBeatmapSetsKey = (query: BeatmapQuery) =>
  ["local-beatmap-sets", query] as const;
export const localSkinsKey = (query: SkinQuery) =>
  ["local-skins", query] as const;

export function useLocalSources() {
  return useQuery({
    queryKey: localSourcesKey,
    queryFn: desktopApi.getLocalSources,
    staleTime: 30_000,
    retry: false,
  });
}

export function useLocalIndexStatus() {
  const queryClient = useQueryClient();
  const result = useQuery({
    queryKey: localIndexStatusKey,
    refetchOnWindowFocus: true,
    refetchIntervalInBackground: false,
    queryFn: desktopApi.getLocalIndexStatus,
    refetchInterval: (query) => {
      const status = query.state.data;
      if (!status || status.phase === "loading") return 1000;
      const watching = Object.values(status.clients ?? {}).some((client) =>
        client?.phase === "pending" || client?.phase === "scanning",
      );
      return watching ? 500 : 5000;
    },
    retry: false,
  });
  useEffect(() => { if (result.data) observeLocalIndex(queryClient, result.data); }, [queryClient, result.data]);
  return result;
}

export function useLocalSummary(client: OsuClient) {
  const indexStatus = useLocalIndexStatus();
  return useQuery({
    queryKey: localSummaryKey(client),
    queryFn: () => desktopApi.getLocalSummary(client),
    enabled: indexStatus.data?.phase === "ready",
    staleTime: 30_000,
    retry: false,
  });
}

export function useLocalBeatmaps(query: BeatmapQuery, enabled: boolean) {
  return useQuery({
    queryKey: localBeatmapsKey(query),
    queryFn: () => desktopApi.queryLocalBeatmaps(query),
    enabled,
    placeholderData: keepPreviousData,
    retry: false,
  });
}

export function useLocalBeatmapSets(query: BeatmapQuery, enabled: boolean) {
  return useQuery({
    queryKey: localBeatmapSetsKey(query),
    queryFn: () => desktopApi.queryLocalBeatmapSets(query),
    enabled,
    placeholderData: keepPreviousData,
    retry: false,
  });
}

export function useLocalBeatmapBackground(
  client: OsuClient,
  resourceId: string | null,
  size: "thumbnail" | "stage" = "thumbnail",
) {
  return useQuery({
    queryKey: ["local-beatmap-background", client, resourceId, size],
    queryFn: () => desktopApi.getLocalBeatmapBackground(client, resourceId!, size),
    enabled: Boolean(resourceId),
    staleTime: Number.POSITIVE_INFINITY,
    retry: false,
  });
}

export function useLocalSkins(query: SkinQuery, enabled: boolean) {
  return useQuery({
    queryKey: localSkinsKey(query),
    queryFn: () => desktopApi.queryLocalSkins(query),
    enabled,
    placeholderData: keepPreviousData,
    retry: false,
  });
}

export function useLocalBeatmapDetail(
  client: OsuClient,
  resourceId: string | null,
) {
  return useQuery({
    queryKey: ["local-beatmap-detail", client, resourceId],
    queryFn: () => desktopApi.getLocalBeatmapDetail(client, resourceId!),
    enabled: Boolean(resourceId),
    retry: false,
  });
}

export function useLocalSkinDetail(
  client: OsuClient,
  resourceId: string | null,
) {
  return useQuery({
    queryKey: ["local-skin-detail", client, resourceId],
    queryFn: () => desktopApi.getLocalSkinDetail(client, resourceId!),
    enabled: Boolean(resourceId),
    retry: false,
  });
}

export function useLocalSkinPreview(
  client: OsuClient,
  resourceId: string | null,
) {
  return useQuery({
    queryKey: ["local-skin-preview", client, resourceId],
    queryFn: () => desktopApi.getLocalSkinPreview(client, resourceId!),
    enabled: Boolean(resourceId),
    retry: false,
  });
}

export function useLocalSkinAsset(
  client: OsuClient,
  skinResourceId: string | null,
  assetResourceId: string | null,
  enabled = true,
) {
  return useQuery({
    queryKey: [
      "local-skin-asset",
      client,
      skinResourceId,
      assetResourceId,
    ],
    queryFn: () =>
      desktopApi.getLocalSkinAsset(
        client,
        skinResourceId!,
        assetResourceId!,
      ),
    enabled:
      enabled && Boolean(skinResourceId) && Boolean(assetResourceId),
    staleTime: Number.POSITIVE_INFINITY,
    retry: false,
  });
}
