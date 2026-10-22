import type { OsuClient, TrainerRequest, TrainerResult, ViewTrainerRequest, ViewTrainerTimeline } from "../../types/osu";
import { call } from "./invoke";

// trainer 域的 Tauri 命令适配（从 lib/tauri.ts 拆分，desktopApi 组合导出不变）。
export const trainerApi = {
  generateTrainerBeatmap: (request: TrainerRequest) =>
    call<TrainerResult>("generate_trainer_beatmap", { request }),

  viewTrainerGetTimeline: (client: OsuClient, resourceId: string) =>
    call<ViewTrainerTimeline>("view_trainer_get_timeline", { client, resourceId }),

  viewTrainerGenerate: (request: ViewTrainerRequest) =>
    call<TrainerResult>("view_trainer_generate", { request }),

  viewTrainerImport: (client: OsuClient, resourceId: string, stagedPath: string) =>
    call<string>("view_trainer_import", { client, resourceId, stagedPath }),
};
