import { useQuery } from "@tanstack/react-query";
import { desktopApi } from "../../shared/lib/tauri";

export const beatmapHubAuthKey = ["beatmaphub-auth"] as const;
export const beatmapHubProfileKey = ["beatmaphub-profile"] as const;

export function useBeatmapHubAuth() {
  return useQuery({ queryKey: beatmapHubAuthKey, queryFn: desktopApi.getBeatmapHubAuthStatus, retry: false, staleTime: 15_000 });
}

export function useBeatmapHubProfile(enabled: boolean) {
  return useQuery({ queryKey: beatmapHubProfileKey, queryFn: desktopApi.getBeatmapHubProfile, enabled, retry: false, staleTime: 15_000 });
}
