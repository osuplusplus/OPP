import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import type { CommandError } from "../../types/osu";
import { isTauri } from "./invoke";

// dialogs 域的 Tauri 命令适配（从 lib/tauri.ts 拆分，desktopApi 组合导出不变）。
export const dialogsApi = {
  chooseSkinAssetFile: async (extension: string) => {
    if (!isTauri()) throw { code: "TAURI_REQUIRED", message: "文件选择器仅可在 OPP 桌面应用中使用" } satisfies CommandError;
    const selected = await openDialog({ multiple: false, title: `选择 .${extension} 替换文件`, filters: [{ name: `${extension.toUpperCase()} 文件`, extensions: [extension] }] });
    return typeof selected === "string" ? selected : null;
  },

  chooseLocalDirectory: async (defaultPath?: string | null, title = "选择 osu! 本地目录") => {
    if (!isTauri()) {
      throw {
        code: "TAURI_REQUIRED",
        message: "目录选择器仅在 OPP 桌面应用中可用",
      } satisfies CommandError;
    }
    const selected = await openDialog({
      directory: true,
      multiple: false,
      defaultPath: defaultPath ?? undefined,
      title,
    });
    return typeof selected === "string" ? selected : null;
  },

  chooseBeatmapPreviewDestination: async (fileName: string, extension: "gif" | "png") => {
    if (!isTauri()) return null;
    const selected = await saveDialog({
      title: "保存铺面预览",
      defaultPath: fileName,
      filters: [{ name: extension === "gif" ? "GIF 动图" : "PNG 图片", extensions: [extension] }],
    });
    return typeof selected === "string" ? selected : null;
  },

  chooseDirectory: async (title: string, defaultPath?: string | null) => {
    if (!isTauri()) {
      return defaultPath ?? "C:/OPP-preview";
    }
    const selected = await openDialog({ directory: true, multiple: false, defaultPath: defaultPath ?? undefined, title });
    return typeof selected === "string" ? selected : null;
  },

  chooseSimilarityBeatmapFile: async () => {
    if (!isTauri()) return "C:/OPP-preview/reference.osu";
    const selected = await openDialog({
      multiple: false,
      title: "选择参考谱面",
      filters: [{ name: "osu! beatmap", extensions: ["osu"] }],
    });
    return typeof selected === "string" ? selected : null;
  },

  chooseTosuExecutable: async (defaultPath?: string | null) => {
    if (!isTauri()) throw { code: "TAURI_REQUIRED", message: "文件选择器仅可在 OPP 桌面应用中使用" } satisfies CommandError;
    const isWindows = navigator.userAgent.includes("Windows");
    const selected = await openDialog({
      multiple: false,
      defaultPath: defaultPath ?? undefined,
      title: isWindows ? "选择 tosu.exe" : "选择 tosu 可执行文件",
      ...(isWindows ? { filters: [{ name: "tosu", extensions: ["exe"] }] } : {}),
    });
    return typeof selected === "string" ? selected : null;
  },

  chooseTosuLyricsExecutable: async (defaultPath?: string | null) => {
    if (!isTauri()) throw { code: "TAURI_REQUIRED", message: "文件选择器仅可在 OPP 桌面应用中使用" } satisfies CommandError;
    const isWindows = navigator.userAgent.includes("Windows");
    const selected = await openDialog({
      multiple: false,
      defaultPath: defaultPath ?? undefined,
      title: isWindows ? "选择 tosu-proxy.exe" : "选择 tosu-proxy 可执行文件",
      ...(isWindows ? { filters: [{ name: "tosu-lyrics", extensions: ["exe"] }] } : {}),
    });
    return typeof selected === "string" ? selected : null;
  },

  chooseManiaBeatmaps: async () => {
    if (!isTauri()) throw { code: "TAURI_REQUIRED", message: "文件选择器仅可在 OPP 桌面应用中使用" } satisfies CommandError;
    const selected = await openDialog({ multiple: true, title: "选择 Malody 谱面", filters: [{ name: "Malody chart", extensions: ["mcz"] }] });
    return Array.isArray(selected) ? selected : typeof selected === "string" ? [selected] : [];
  },

  chooseBeatmapDownloadDirectory: async (defaultPath?: string | null) => {
    if (!isTauri()) {
      throw {
        code: "TAURI_REQUIRED",
        message: "目录选择器仅可在 OPP 桌面应用中使用",
      } satisfies CommandError;
    }
    const selected = await openDialog({
      directory: true,
      multiple: false,
      defaultPath: defaultPath ?? undefined,
      title: "选择谱面下载目录",
    });
    return typeof selected === "string" ? selected : null;
  },

  chooseFfmpegExecutable: async (defaultPath?: string | null) => {
    if (!isTauri()) return null;
    const isWindows = navigator.userAgent.includes("Windows");
    const selected = await openDialog({
      directory: false,
      multiple: false,
      defaultPath: defaultPath ?? undefined,
      title: isWindows ? "选择 ffmpeg.exe" : "选择 ffmpeg 可执行文件",
      ...(isWindows ? { filters: [{ name: "FFmpeg", extensions: ["exe"] }] } : {}),
    });
    return typeof selected === "string" ? selected : null;
  },
};
