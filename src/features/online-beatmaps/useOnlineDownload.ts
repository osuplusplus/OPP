import { useQueryClient } from "@tanstack/react-query";
import { desktopApi } from "../../shared/lib/tauri";
import type { BeatmapDownloadProvider, OnlineBeatmapset } from "../../shared/types/osu";
import { settingsQueryKey, useSettings } from "../settings/api";
import { beatmapDownloadDirectoryKey, downloadSession, useDownloadSession } from "./downloadSession";
import { resolveDefaultDownloadProvider } from "./downloadProvider";

export function useOnlineDownload() {
  const settings = useSettings();
  const queryClient = useQueryClient();
  const state = useDownloadSession();
  const destination = settings.data?.beatmap_download_directory || localStorage.getItem(beatmapDownloadDirectoryKey) || "";
  const saveDestination = async (path: string) => {
    localStorage.setItem(beatmapDownloadDirectoryKey, path);
    if (settings.data) queryClient.setQueryData(settingsQueryKey, await desktopApi.updateSettings({ ...settings.data, beatmap_download_directory: path }));
  };
  const start = (items: OnlineBeatmapset[], options?: { destination?: string; provider?: BeatmapDownloadProvider | "none"; overwrite?: boolean }) => downloadSession.start(items, async () => {
    const provider = options?.provider ?? resolveDefaultDownloadProvider(settings.data);
    if (provider === "none") throw new Error("请在下载清单的更多选项中选择下载源。");
    let path = options?.destination || destination;
    if (!path) {
      path = await desktopApi.chooseBeatmapDownloadDirectory(null) ?? "";
      if (!path) return null;
      await saveDestination(path);
    }
    return { destination: path, provider, overwrite: options?.overwrite ?? false, include_video: settings.data?.include_video_in_beatmap_downloads ?? true };
  });
  return { state, start, destination, saveDestination, defaultProvider: resolveDefaultDownloadProvider(settings.data) };
}
