import { desktopApi } from "../../shared/lib/tauri";
import type { OsuClient, ViewTrainerRequest } from "../../shared/types/osu";

// view-trainer 的命令边界：页面不直调 trainer 命令。
// 实时预览的 liveRender* 会话控制（open/pause/seek/close）属于 live-render 域，仍由页面直调。
export const viewTrainerApi = {
  getTimeline: (client: OsuClient, resourceId: string) =>
    desktopApi.viewTrainerGetTimeline(client, resourceId),
  generate: (request: ViewTrainerRequest) =>
    desktopApi.viewTrainerGenerate(request),
  importBeatmap: (
    client: OsuClient,
    resourceId: string,
    stagedDirectory: string,
  ) => desktopApi.viewTrainerImport(client, resourceId, stagedDirectory),
};
