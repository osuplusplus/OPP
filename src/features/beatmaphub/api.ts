import { useQuery } from "@tanstack/react-query";
import { desktopApi } from "../../shared/lib/tauri";

export const beatmapHubAuthKey = ["beatmaphub-auth"] as const;
export const beatmapHubProfileKey = ["beatmaphub-profile"] as const;
export const beatmapHubRecommendationsKey = ["beatmaphub-recommendations"] as const;

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
    retry: 1,
    staleTime: 5 * 60_000,
  });
}
