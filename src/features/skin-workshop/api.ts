import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { desktopApi } from "../../shared/lib/tauri";
import type { OsuClient } from "../../shared/types/osu";

export const workshopTreeKey = (client: OsuClient, skinResourceId: string) =>
  ["skin-workshop-tree", client, skinResourceId] as const;
export const workshopPartKey = (client: OsuClient, skinResourceId: string, partKey: string) =>
  ["skin-workshop-part", client, skinResourceId, partKey] as const;

export function useWorkshopTree(client: OsuClient, skinResourceId: string | null) {
  return useQuery({
    queryKey: workshopTreeKey(client, skinResourceId ?? ""),
    queryFn: () => desktopApi.getSkinWorkshopTree(client, skinResourceId!),
    enabled: client === "stable" && Boolean(skinResourceId),
    placeholderData: keepPreviousData,
  });
}

export function useWorkshopPart(client: OsuClient, skinResourceId: string | null, partKey: string | null) {
  return useQuery({
    queryKey: workshopPartKey(client, skinResourceId ?? "", partKey ?? ""),
    queryFn: () => desktopApi.getSkinWorkshopPartPreview(client, skinResourceId!, partKey!),
    enabled: client === "stable" && Boolean(skinResourceId && partKey),
  });
}

export function useWorkshopConfig(client: OsuClient, skinResourceId: string | null, enabled = true) {
  return useQuery({
    queryKey: ["skin-workshop-config", client, skinResourceId],
    queryFn: () => desktopApi.getSkinWorkshopConfig(client, skinResourceId!),
    enabled: enabled && Boolean(skinResourceId),
  });
}

export function useWorkshopAsset(client: OsuClient, skinResourceId: string, assetId: string, enabled = true) {
  return useQuery({
    queryKey: ["skin-workshop-asset", client, skinResourceId, assetId],
    queryFn: () => desktopApi.getSkinWorkshopAsset(client, skinResourceId, assetId),
    enabled,
    staleTime: Number.POSITIVE_INFINITY,
  });
}
