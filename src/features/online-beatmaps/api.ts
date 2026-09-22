import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { desktopApi } from "../../shared/lib/tauri";
import type { OnlineBeatmapSearchQuery, Ruleset } from "../../shared/types/osu";
import { TREND_WINDOW_DAYS, trendingQuery } from "./stageModel";

export { useOnlineDownload as useBeatmapDownloads } from "./useOnlineDownload";
export { DownloadResultActions } from "./DownloadResultActions";
export type { BeatmapDownloadSelection } from "./downloadSession";

export function useTrendingBeatmapsets(ruleset: Ruleset) {
  return useQuery({
    queryKey: ["online-trending", ruleset, TREND_WINDOW_DAYS],
    queryFn: async () => (await desktopApi.searchOnlineBeatmapsets(trendingQuery(ruleset))).beatmapsets.slice(0, 5),
    staleTime: 15 * 60_000,
    gcTime: 30 * 60_000,
    retry: false,
  });
}

export const onlineBeatmapsKey = (query: OnlineBeatmapSearchQuery) =>
  ["online-beatmaps", query] as const;

export function useOnlineBeatmapsets(
  query: OnlineBeatmapSearchQuery,
  enabled: boolean,
) {
  return useInfiniteQuery({
    queryKey: onlineBeatmapsKey(query),
    queryFn: ({ pageParam }) =>
      desktopApi.searchOnlineBeatmapsets({
        ...query,
        cursor_string: pageParam,
      }),
    enabled,
    staleTime: 60_000,
    refetchOnMount: false,
    refetchOnWindowFocus: false,
    initialPageParam: null as string | null,
    getNextPageParam: (page) => page.cursor_string || undefined,
    retry: false,
  });
}

export function useOnlineBeatmapsetDetail(beatmapsetId: number | null) {
  return useQuery({
    queryKey: ["online-beatmapset", beatmapsetId],
    queryFn: () => desktopApi.getOnlineBeatmapset(beatmapsetId!),
    enabled: beatmapsetId !== null,
    staleTime: 5 * 60_000,
    retry: false,
  });
}

export function useOnlineBeatmapProviderStatus() {
  return useQuery({
    queryKey: ["online-beatmap-providers"],
    queryFn: () => desktopApi.getOnlineBeatmapProviderStatus(),
    staleTime: 60_000,
    retry: false,
  });
}
