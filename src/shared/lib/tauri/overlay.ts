import type {
  OverlayChatEntry,
  OverlayChatUpdate,
  OverlayIngameInstallInfo,
  OverlayMpActivity,
  OverlayMpStatus,
  OverlayState,
} from "../../types/osu";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { call, isTauri } from "./invoke";

// overlay 域的 Tauri 命令适配（从 lib/tauri.ts 拆分，desktopApi 组合导出不变）。
export const overlayApi = {
  overlayGetState: () => call<OverlayState>("overlay_get_state"),

  overlaySetMode: (mode: "hidden" | "hud" | "panel") =>
    call<void>("overlay_set_mode", { mode }),

  /** 收起面板（Esc/关闭按钮），回到唤出前的状态（HUD 或隐藏）。 */
  overlayClosePanel: () => call<void>("overlay_close_panel"),

  overlaySetVisible: (visible: boolean) => call<void>("overlay_set_visible", { visible }),

  overlayTestHotkey: (hotkey: string) => call<void>("overlay_test_hotkey", { hotkey }),

  overlayGetMpStatus: () => call<OverlayMpStatus>("overlay_get_mp_status"),

  overlaySetIrcCredentials: (username: string | null, token: string) =>
    call<void>("overlay_set_irc_credentials", { username, token }),

  overlayInstallIngame: () =>
    call<OverlayIngameInstallInfo>("overlay_install_ingame"),

  overlayUninstallIngame: () => call<void>("overlay_uninstall_ingame"),

  /** 读取 MP 频道聊天缓冲（时间正序），limit 缺省 100、上限 300。 */
  overlayGetChat: (limit?: number) =>
    call<OverlayChatEntry[]>("overlay_get_chat", { limit: limit ?? null }),

  /** 把译文写入系统剪贴板（复制到游戏聊天粘贴发送）。 */
  overlayCopyText: (text: string) => call<void>("overlay_copy_text", { text }),

  onOverlayModeChanged: async (
    handler: (mode: "hidden" | "hud" | "panel") => void,
  ): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<"hidden" | "hud" | "panel">("overlay-mode-changed", (event) =>
      handler(event.payload),
    );
  },

  onOverlayMpActivity: async (
    handler: (activity: OverlayMpActivity) => void,
  ): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<OverlayMpActivity>("overlay-mp-activity", (event) =>
      handler(event.payload),
    );
  },

  onOverlayChatMessage: async (
    handler: (entry: OverlayChatEntry) => void,
  ): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<OverlayChatEntry>("overlay-chat-message", (event) =>
      handler(event.payload),
    );
  },

  onOverlayChatUpdated: async (
    handler: (update: OverlayChatUpdate) => void,
  ): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<OverlayChatUpdate>("overlay-chat-updated", (event) =>
      handler(event.payload),
    );
  },
};
