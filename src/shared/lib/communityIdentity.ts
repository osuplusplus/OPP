import { useQuery } from "@tanstack/react-query";
import { desktopApi } from "./tauri";

// One query/session across BeatmapHub and community pages.
export const communityIdentityKey = ["beatmaphub", "auth"] as const;
export const communityProfileKey = ["beatmaphub", "profile"] as const;
export function useCommunityIdentity() {
  return useQuery({ queryKey: communityIdentityKey, queryFn: desktopApi.getBeatmapHubAuthStatus, gcTime: 0 });
}
export function useCommunityProfile(enabled: boolean) {
  return useQuery({ queryKey: communityProfileKey, queryFn: desktopApi.getBeatmapHubProfile, enabled, gcTime: 0 });
}
export async function connectCommunityIdentity() {
  try { return await desktopApi.loginBeatmapHub(); }
  catch (error) {
    if ((error as { code?: string }).code === "DEVICE_REVOKED") return desktopApi.reconnectBeatmapHub();
    throw error;
  }
}
