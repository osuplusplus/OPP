import { useQuery } from "@tanstack/react-query";
import { localDataApi } from "./localData";
import { tournamentPoolsApi } from "./tournamentPools";
import { collectionsApi } from "./collections";
import { logsApi } from "./logs";
import { authApi } from "./auth";
import { communityApi } from "./community";
import { beatmaphubApi } from "./beatmaphub";
import { systemToolsApi } from "./systemTools";
import { profileApi } from "./profile";
import { onlineBeatmapsApi } from "./onlineBeatmaps";
import { updatesApi } from "./updates";
import { settingsApi } from "./settings";
import { gameMediaApi } from "./gameMedia";
import { trainerApi } from "./trainer";
import { liveIntegrationApi } from "./liveIntegration";
import { localAnalysisApi } from "./localAnalysis";
import { similarityApi } from "./similarity";
import { dialogsApi } from "./dialogs";
import { skinWorkshopApi } from "./skinWorkshop";
import { shellApi } from "./shell";
import { liveRenderApi } from "./liveRender";
import { overlayApi } from "./overlay";

// desktopApi 组合入口：各域适配器聚合，消费方仍从 "shared/lib/tauri" 导入。
export const desktopApi = {
  ...localDataApi,
  ...tournamentPoolsApi,
  ...collectionsApi,
  ...logsApi,
  ...authApi,
  ...communityApi,
  ...beatmaphubApi,
  ...systemToolsApi,
  ...profileApi,
  ...onlineBeatmapsApi,
  ...updatesApi,
  ...settingsApi,
  ...gameMediaApi,
  ...trainerApi,
  ...liveIntegrationApi,
  ...localAnalysisApi,
  ...similarityApi,
  ...dialogsApi,
  ...skinWorkshopApi,
  ...shellApi,
  ...liveRenderApi,
  ...overlayApi,
};


export const capabilitiesQueryKey = ["platform-capabilities"] as const;

/**
 * 平台能力（由后端 `get_capabilities` 用 `cfg` 一次性算好下发）。业务页面面向
 * 能力判断，而非直接判断操作系统 —— 新增平台只改后端 `platform` 模块。
 */
export function useCapabilities() {
  return useQuery({
    queryKey: capabilitiesQueryKey,
    queryFn: () => desktopApi.getCapabilities(),
    staleTime: Infinity,
  });
}

export type { LiveExportParams, LiveRenderOptions, LiveSkinEntry } from "./liveRender";
