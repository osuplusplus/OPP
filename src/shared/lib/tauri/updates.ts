import type { AppSettings } from "../../types/osu";
import { UpdateCheckResult, call } from "./invoke";

// updates 域的 Tauri 命令适配（从 lib/tauri.ts 拆分，desktopApi 组合导出不变）。
export const updatesApi = {
  checkForUpdates: () => call<UpdateCheckResult>("check_for_updates"),

  downloadAndInstallUpdate: (expectedVersion: string) =>
    call<void>("download_and_install_update", { expectedVersion }),

  ignoreUpdateVersion: (version: string) =>
    call<AppSettings>("ignore_update_version", { version }),
};
