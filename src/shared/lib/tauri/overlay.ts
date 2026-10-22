import type { OverlayIngameInstallInfo, OverlayMpActivity, OverlayMpStatus, OverlayState } from "../../types/osu";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { call, isTauri } from "./invoke";

// overlay 域的 Tauri 命令适配（从 lib/tauri.ts 拆分，desktopApi 组合导出不变）。
export const overlayApi = {
  overlayGetState: () => call<OverlayState>("overlay_get_state"),

  overlaySetMode: (mode: "hud" | "panel") => call<void>("overlay_set_mode", { mode }),

  overlaySetVisible: (visible: boolean) => call<void>("overlay_set_visible", { visible }),

  overlayTestHotkey: (hotkey: string) => call<void>("overlay_test_hotkey", { hotkey }),

  overlayGetMpStatus: () => call<OverlayMpStatus>("overlay_get_mp_status"),

  overlaySetIrcCredentials: (username: string | null, token: string) =>
    call<void>("overlay_set_irc_credentials", { username, token }),

  overlayInstallIngame: () =>
    call<OverlayIngameInstallInfo>("overlay_install_ingame"),

  overlayUninstallIngame: () => call<void>("overlay_uninstall_ingame"),

  onOverlayModeChanged: async (handler: (mode: "hud" | "panel") => void): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<"hud" | "panel">("overlay-mode-changed", (event) => handler(event.payload));
  },

  onOverlayMpActivity: async (
    handler: (activity: OverlayMpActivity) => void,
  ): Promise<UnlistenFn> => {
    if (!isTauri()) return () => undefined;
    return listen<OverlayMpActivity>("overlay-mp-activity", (event) =>
      handler(event.payload),
    );
  },
};
