import type { CommandError } from "../../types/osu";
import { invoke } from "@tauri-apps/api/core";
import { measureCommand } from "../performance";
import { browserPreviewValue } from "./preview";

// 命令传输核心：isTauri 判定、错误规范化、日志与 call() 适配。
export const isTauri = () => "__TAURI_INTERNALS__" in window;

export interface UpdateCheckResult {
  current_version: string;
  latest_version: string;
  latest_tag: string;
  is_latest: boolean;
  release_name: string | null;
  release_url: string;
  published_at: string | null;
  release_notes: string | null;
  can_auto_update: boolean;
  download_size: number | null;
}

export interface UpdateProgress {
  phase: "downloading" | "preparing" | "restarting";
  downloaded_bytes: number;
  total_bytes: number;
  message: string;
}

export function normalizeError(error: unknown): CommandError {
  if (typeof error === "object" && error && "code" in error && "message" in error) {
    return error as CommandError;
  }
  if (typeof error === "string") {
    try {
      const parsed = JSON.parse(error);
      if (parsed?.code && parsed?.message) return parsed;
    } catch {
      return { code: "UNKNOWN_ERROR", message: error };
    }
    return { code: "UNKNOWN_ERROR", message: error };
  }
  return {
    code: "UNKNOWN_ERROR",
    message: error instanceof Error ? error.message : "发生未知错误",
  };
}

export function requestId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) return crypto.randomUUID();
  return `frontend-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

export async function writeLogRaw(level: string, target: string, message: string): Promise<void> {
  if (!isTauri()) return;
  try {
    await invoke<void>("write_client_log", { level, target, message });
  } catch {
    // Logging must never turn an otherwise valid command into a UI error.
  }
}

export async function call<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!isTauri()) {
    const preview = browserPreviewValue<T>(command, args);
    if (preview !== undefined) return preview;
    throw {
      code: "TAURI_REQUIRED",
      message: "请通过 OPP 桌面应用运行此功能",
    } satisfies CommandError;
  }
  const id = requestId();
  const finish = measureCommand(command);
  try {
    const result = await invoke<T>(command, args);
    finish(result);
    return result;
  } catch (error) {
    finish();
    const normalized = normalizeError(error);
    void writeLogRaw("error", "frontend.command.return_err", JSON.stringify({ event: "return_err", command, request_id: id, code: normalized.code, message: normalized.message, backend_request_id: normalized.request_id }));
    throw normalized;
  }
}
