import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type { DanserEnqueueRequest, DanserRenderJob, DanserStatus, GameMediaItem, GameReplayPayload, GameScreenshotPayload, GameSessionSummary, GameStatusSnapshot, ManiaConversionResult, NewReplaysDetected, OsuClient, ReplayFileSelection, ReplayMapInfo, ReplayRenderJob, ReplayRenderProgress, ReplayRenderRequest, Ruleset } from "../../types/osu";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { call, isTauri } from "./invoke";

// gameMedia 域的 Tauri 命令适配（从 lib/tauri.ts 拆分，desktopApi 组合导出不变）。
export const gameMediaApi = {
  startGameSession: (ruleset: Ruleset, client: OsuClient, launchTosu?: boolean) =>
    call<GameSessionSummary>("start_game_session", { ruleset, client, launchTosu }),

  startDetectedGameSession: (ruleset: Ruleset, client: OsuClient) =>
    call<GameSessionSummary>("start_detected_game_session", { ruleset, client }),

  getGameSessionStatus: () =>
    call<GameSessionSummary | null>("get_game_session_status"),

  getGameStatus: () => call<GameStatusSnapshot>("get_game_status"),

  convertManiaBeatmaps: (paths: string[]) =>
    call<ManiaConversionResult>("convert_mania_beatmaps", { paths }),

  chooseGameReplayFiles: (client: OsuClient) => call<ReplayFileSelection>("choose_game_replay_files", { client }),

  listGameMedia: (client: OsuClient) => call<GameMediaItem[]>("list_game_media", { client }),

  readGameReplay: (client: OsuClient, path: string) =>
    call<GameReplayPayload>("read_game_replay", { client, path }),

  inspectGameReplay: (client: OsuClient, path: string) =>
    call<ReplayMapInfo>("inspect_game_replay", { client, path }),

  readGameScreenshot: (client: OsuClient, path: string) =>
    call<GameScreenshotPayload>("read_game_screenshot", { client, path }),

  submitReplayRender: (request: ReplayRenderRequest) =>
    call<ReplayRenderJob>("submit_replay_render", { request }),

  getDanserStatus: () => call<DanserStatus>("get_danser_status"),

  listDanserProfiles: () => call<string[]>("list_danser_profiles"),

  enqueueDanserRenders: (request: DanserEnqueueRequest) =>
    call<DanserRenderJob[]>("enqueue_danser_renders", { request }),

  startDanserRenderQueue: () => call<void>("start_danser_render_queue"),

  getDanserRenderQueue: () => call<DanserRenderJob[]>("get_danser_render_queue"),

  cancelDanserRender: (id: string) => call<void>("cancel_danser_render", { id }),

  openDanserOutput: (path: string) => call<void>("open_danser_output", { path }),

  chooseDanserExecutable: async (defaultPath?: string | null) => {
    if (!isTauri()) return null;
    const isWindows = navigator.userAgent.includes("Windows");
    const selected = await openDialog({
      directory: false,
      multiple: false,
      defaultPath: defaultPath ?? undefined,
      title: isWindows ? "选择 danser-cli.exe" : "选择 danser 可执行文件",
      ...(isWindows ? { filters: [{ name: "Danser CLI", extensions: ["exe"] }] } : {}),
    });
    return typeof selected === "string" ? selected : null;
  },

  exportReplayVideo: (videoUrl: string, fileName: string) =>
    call<string>("export_replay_video", { videoUrl, fileName }),

  onReplayRenderProgress: async (
    handler: (progress: ReplayRenderProgress) => void,
  ): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<ReplayRenderProgress>("ordr-render-progress", (event) =>
      handler(event.payload),
    );
  },

  onDanserRenderProgress: async (
    handler: (progress: DanserRenderJob) => void,
  ): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<DanserRenderJob>("danser-render-progress", (event) => handler(event.payload));
  },

  onNewReplaysDetected: async (
    handler: (payload: NewReplaysDetected) => void,
  ): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<NewReplaysDetected>("new-replays-detected", (event) => handler(event.payload));
  },

  onGameStatusChanged: async (
    handler: (status: GameStatusSnapshot) => void,
  ): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<GameStatusSnapshot>("game-status-changed", (event) => handler(event.payload));
  },
};
