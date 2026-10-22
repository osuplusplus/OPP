import type { AppSettings } from "../../types/osu";
import { call } from "./invoke";

// settings 域的 Tauri 命令适配（从 lib/tauri.ts 拆分，desktopApi 组合导出不变）。
export const settingsApi = {
  getSettings: () => call<AppSettings>("get_settings"),

  updateSettings: (settings: AppSettings) =>
    call<AppSettings>("update_settings", { settings }),

  markOnboardingSeen: (version: number) =>
    call<AppSettings>("mark_onboarding_seen", { version }),

  markPageOnboardingSeen: (pageId: string, version: number) =>
    call<AppSettings>("mark_page_onboarding_seen", { pageId, version }),
};
