import type { LogFileInfo, LogLevel } from "../../types/osu";
import { call, writeLogRaw } from "./invoke";

// logs 域的 Tauri 命令适配（从 lib/tauri.ts 拆分，desktopApi 组合导出不变）。
export const logsApi = {
  getLogDirectory: () => call<string>("get_log_directory"),

  listLogFiles: () => call<LogFileInfo[]>("list_log_files"),

  openLogDirectory: () => call<void>("open_log_directory"),

  openLogFile: (name: string) => call<void>("open_log_file", { name }),

  writeClientLog: (level: LogLevel, target: string, message: string) => writeLogRaw(level, target, message),
};
