import type { OAuthResult } from "../../types/osu";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { UpdateProgress, isTauri } from "./invoke";

// shell 域的 Tauri 命令适配（从 lib/tauri.ts 拆分，desktopApi 组合导出不变）。
export const shellApi = {
  openExternal: async (url: string) => {
    // 与 opener 插件能力白名单保持一致：只放行网页与 mailto/tel，拦截其他 scheme。
    let scheme: string | null = null;
    try {
      scheme = new URL(url).protocol.toLowerCase();
    } catch {
      // 解析失败保持 null，按无 scheme 拦截。
    }
    if (scheme !== "http:" && scheme !== "https:" && scheme !== "mailto:" && scheme !== "tel:") {
      console.warn(`[openExternal] 已拦截非网页链接：${url}`);
      return;
    }
    if (isTauri()) await openUrl(url);
    else window.open(url, "_blank", "noopener,noreferrer");
  },

  onOAuthResult: async (
    handler: (result: OAuthResult) => void,
  ): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<OAuthResult>("oauth-result", (event) => handler(event.payload));
  },

  onUpdateProgress: async (
    handler: (progress: UpdateProgress) => void,
  ): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<UpdateProgress>("update-progress", (event) => handler(event.payload));
  },
};
