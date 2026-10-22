import type { FfmpegStatusInfo, OsuClient } from "../../types/osu";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { call, isTauri } from "./invoke";

export interface LiveExportParams {
  outPath: string;
  width: number;
  height: number;
  fps: number;
  encoder: "x264" | "x265" | "nvenc" | "hevc_nvenc";
  quality: number;
  audio: boolean;
  hitsounds: boolean;
  /** 结算屏（lazer ResultsScreen,玩法结束后追加 4 秒）,默认开。 */
  results: boolean;
  /** 导出专用 BGM 偏移 ms(与预览偏移独立,默认 0)。 */
  audioOffset: number;
}

export interface LiveRenderOptions {
  /** 玩法 HUD 总开关(预览与导出共用;off 隐藏整个 HUD,物件/光标照常)。 */
  hud: boolean;
  urBar: boolean;
  followPoints: boolean;
  keyOverlay: boolean;
  /** 实时 PP 计数器(逐物件渐增,Argon 样式挂 ACC 行下方;后端字段 ppDisplay)。 */
  ppDisplay: boolean;
  /** 谱面背景图,默认开。 */
  bg: boolean;
  /** 故事板渲染(.osu Events + 共享 .osb;切换重建会话)。 */
  storyboard: boolean;
  /** 背景视频(故事板 Video 元素,ffmpeg 管道解码)。 */
  video: boolean;
  /** 背景亮度 0..1,独立常驻选项:同时作用于背景图/故事板/背景视频(拖动即时生效)。 */
  bgOpacity: number;
  audio: boolean;
  audioOffset: number;
  hitsounds: boolean;
  /** 光标尺寸倍率 0.1..2(lazer GameplayCursorSize,默认 1)。 */
  cursorSize: number;
  /** 用户皮肤目录路径;null = 内置 Argon-Pro(后端字段 skinPath)。 */
  skinPath: string | null;
  /** 强制用皮肤 combo 色覆盖谱面 [Colours](stable 行为,默认关:谱面色优先)。 */
  skinColours: boolean;
  /** 结算屏头像落盘路径(resolveAvatarFile 产物);null = 首字母占位。 */
  avatarPath: string | null;
}

export interface LiveSkinEntry {
  name: string;
  path: string;
}

// liveRender 域的 Tauri 命令适配（从 lib/tauri.ts 拆分，desktopApi 组合导出不变）。
export const liveRenderApi = {
  liveRenderOpen: (
    beatmapPath: string,
    replayPath: string,
    options: LiveRenderOptions,
    rect: { x: number; y: number; width: number; height: number; viewport_width?: number; viewport_height?: number },
  ) =>
    call<{ durationMs: number }>("live_render_open", {
      beatmapPath,
      replayPath,
      options,
      rect,
    }),

  /** 以谱面 Autoplay 打开实时预览时传入空 replayPath。 */
  liveRenderMove: (rect: { x: number; y: number; width: number; height: number; viewport_width?: number; viewport_height?: number; suppressed?: boolean }) =>
    call<void>("live_render_move", { rect }),

  liveRenderSeek: (timeMs: number) => call<void>("live_render_seek", { timeMs }),

  liveRenderSetOptions: (options: LiveRenderOptions) =>
    call<void>("live_render_set_options", { options }),

  liveRenderResolveAvatar: (userId: number, avatarUrl: string) =>
    call<string | null>("resolve_avatar_file", { userId, avatarUrl }),

  liveRenderCheckFfmpeg: () => call<string | null>("live_render_check_ffmpeg"),

  liveRenderListSkins: (client: OsuClient) => call<LiveSkinEntry[]>("live_render_list_skins", { client }),

  liveRenderCheckNvenc: () => call<[boolean, boolean]>("live_render_check_nvenc"),

  liveRenderGetFfmpegStatus: () => call<FfmpegStatusInfo>("live_render_get_ffmpeg_status"),

  liveRenderExport: (
    beatmapPath: string,
    replayPath: string,
    options: LiveRenderOptions,
    params: LiveExportParams,
  ) =>
    call<string>("live_render_export", {
      beatmapPath,
      replayPath,
      options,
      params,
    }),

  liveRenderExportCancel: () => call<void>("live_render_export_cancel"),

  liveRenderOpenExportOutput: (path: string) =>
    call<void>("live_render_open_export_output", { path }),

  onLiveRenderExport: async (
    handler: (progress: { phase: string; frame: number; total: number; message: string }) => void,
  ): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<{
      phase: string;
      frame: number;
      total: number;
      message: string;
    }>("live-render-export", (event) => handler(event.payload));
  },

  liveRenderPlay: () => call<void>("live_render_play"),

  liveRenderPause: () => call<void>("live_render_pause"),

  liveRenderClose: () => call<void>("live_render_close"),

  onLiveRenderTime: async (
    handler: (state: { active: boolean; playing: boolean; timeMs: number; durationMs: number }) => void,
  ): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<{
      active: boolean;
      playing: boolean;
      time_ms: number;
      duration_ms: number;
    }>("live-render-time", (event) =>
      handler({
        active: event.payload.active,
        playing: event.payload.playing,
        timeMs: event.payload.time_ms,
        durationMs: event.payload.duration_ms,
      }),
    );
  },

  /** 皮肤热切换失败(加载错误;当前皮肤保持不变)。 */
  onLiveRenderSkinError: async (handler: (message: string) => void): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<string>("live-render-skin-error", (event) => handler(event.payload));
  },

  /** 渲染线程异常:会话已被后端清理(停播 + 销毁窗口),UI 需复位。 */
  onLiveRenderError: async (handler: (message: string) => void): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<string>("live-render-error", (event) => handler(event.payload));
  },
};
