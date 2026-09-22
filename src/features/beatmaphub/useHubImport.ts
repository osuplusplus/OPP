import { useEffect, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import type { BeatmapHubImportResult, OnlineBeatmapset } from "../../shared/types/osu";
import { resolveDefaultDownloadProvider } from "../online-beatmaps/downloadProvider";
import { hubApi, hubPackKey, type useHubDownloadConfig } from "./api";
import { hubError } from "./model";

export interface HubImportTask {
  id: string;
  title: string;
  phase: "importing" | "checking" | "downloading" | "installing" | "complete" | "error";
  imported?: BeatmapHubImportResult;
  message: string;
  error?: string;
  pendingPaths?: string[];
  downloadIncomplete?: boolean;
}
type DownloadConfig = NonNullable<ReturnType<typeof useHubDownloadConfig>["data"]>;
export function useHubImport() {
  const client = useQueryClient();
  const [tasks, setTasks] = useState<Record<string, HubImportTask>>({});
  const tasksRef = useRef<Record<string, HubImportTask>>({});
  const activeId = useRef<string | null>(null);
  const patch = (id: string, value: Partial<HubImportTask>) => {
    tasksRef.current = { ...tasksRef.current, [id]: { ...tasksRef.current[id], ...value } };
    setTasks(tasksRef.current);
  };
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void hubApi.onDownloadProgress((progress) => {
      const id = activeId.current;
      if (!id || tasksRef.current[id]?.phase !== "downloading") return;
      patch(id, { message: `已创建收藏夹 · 下载 ${progress.processed}/${progress.total}${progress.current_title ? `：${progress.current_title}` : ""}` });
    }).then((stop) => { if (disposed) stop(); else unlisten = stop; }).catch(() => { /* Completion/error feedback still comes from the command result. */ });
    return () => { disposed = true; unlisten?.(); };
  }, []);

  const run = async (id: string, title: string, resolved: OnlineBeatmapset[], download: boolean, config: DownloadConfig | undefined) => {
    // Collection downloads use a shared backend task; never overlap Hub imports.
    if (activeId.current || tasksRef.current[id]?.phase === "complete") return;
    activeId.current = id;
    const previous = tasksRef.current[id];
    patch(id, { id, title, phase: previous?.imported ? "checking" : "importing", message: previous?.imported ? "正在重试补齐，保留已有收藏夹…" : "正在创建收藏夹…", error: undefined });
    try {
      let imported = previous?.imported;
      if (!imported) {
        imported = await hubApi.importPack(id, resolved);
        // Save before any subsequent await so every retry reuses this folder.
        patch(id, { imported, message: `已导入 ${imported.imported_sets} 个谱面集` });
        void client.invalidateQueries({ queryKey: ["collections"] });
      }
      if (download) {
        if (!config?.root) throw new Error("收藏夹已创建。请配置有效的 osu!stable 目录后重试下载。");
        await hubApi.beginTask();
        let paths = previous?.pendingPaths ?? [];
        let incomplete = previous?.downloadIncomplete ?? false;
        if (paths.length) {
          patch(id, { phase: "installing", message: "已创建收藏夹 · 正在重试安装已下载的谱面…" });
          const install = await hubApi.install([imported.folder_id], paths);
          patch(id, { pendingPaths: [] });
          if (install.unresolved_entries) throw new Error(`收藏夹已保留，仍有 ${install.unresolved_entries} 个条目未解析，请重试补齐。`);
          paths = [];
        }
        patch(id, { phase: "checking", message: "已创建收藏夹 · 正在检查缺失谱面…" });
        const items = await hubApi.downloadItems([imported.folder_id]);
        if (items.length) {
          patch(id, { phase: "downloading", message: `已创建收藏夹 · 正在下载 ${items.length} 个谱面集…` });
          const root = config.root.replace(/[\\/]+$/, "");
          const separator = root.includes("\\") ? "\\" : "/";
          const result = await hubApi.download({
            destination: config.settings.beatmap_download_directory || `${root}${separator}OPP Downloads`,
            provider: resolveDefaultDownloadProvider(config.settings), overwrite: false,
            include_video: config.settings.include_video_in_beatmap_downloads, open_after_download: false, items,
          });
          paths = result.completed_paths ?? [];
          incomplete = result.failed > 0 || result.cancelled;
          patch(id, { pendingPaths: paths, downloadIncomplete: incomplete });
          if (paths.length) {
            patch(id, { phase: "installing", message: `下载成功 ${result.completed}，失败 ${result.failed} · 正在安装 ${paths.length} 个谱面集…` });
            const install = await hubApi.install([imported.folder_id], paths);
            patch(id, { pendingPaths: [] });
            if (install.unresolved_entries) throw new Error(`收藏夹已保留，仍有 ${install.unresolved_entries} 个条目未解析，请重试补齐。`);
          } else if (!incomplete) throw new Error("下载未返回可安装的文件，请重试补齐。");
          if (incomplete) throw new Error(`收藏夹已保留；下载${result.cancelled ? "已取消" : `失败 ${result.failed} 个`}，可重试补齐。`);
        }
      }
      patch(id, { phase: "complete", message: `已导入 ${imported.imported_sets} 个谱面集${imported.unresolved_sets ? `（${imported.unresolved_sets} 个以占位条目导入）` : ""}${download ? " · 缺失谱面已补齐" : " · 已保存到本地收藏夹"}` });
    } catch (error) {
      patch(id, { phase: "error", error: hubError(error), message: tasksRef.current[id]?.imported ? "收藏夹已创建，补齐未完成" : "导入未完成" });
    } finally {
      activeId.current = null;
      void client.invalidateQueries({ queryKey: ["collections"] });
      void client.invalidateQueries({ queryKey: hubPackKey });
    }
  };
  const busy = Object.values(tasks).some((task) => task.phase !== "complete" && task.phase !== "error");
  return { tasks, busy, run };
}
