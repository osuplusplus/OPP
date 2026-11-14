import { useEffect, useState } from "react";
import { desktopApi } from "../../shared/lib/tauri";
import type {
  BeatmapDownloadProgress,
  TosuLiveSnapshot,
} from "../../shared/types/osu";
import { createDownloadProgressDisplay } from "../online-beatmaps/downloadProgressDisplay";

/**
 * tosu 实时快照订阅:事件频率与游戏帧率同级,只应在真正渲染
 * 快照数据的叶子组件中调用,避免高频 setState 扩散到整棵面板树。
 */
export function useTosuSnapshot(): TosuLiveSnapshot | null {
  const [snapshot, setSnapshot] = useState<TosuLiveSnapshot | null>(null);
  useEffect(() => {
    const pending = desktopApi.onTosuLiveData(setSnapshot);
    return () => {
      void pending.then((unlisten) => unlisten());
    };
  }, []);
  return snapshot;
}

/**
 * 谱面下载进度订阅。finished/cancelled 后清空,恢复为"无下载"状态。
 * 并行下载(最多同时 2 个)会交替上报各自的曲名,经状态适配器固定为
 * 一个进行中的条目,避免 HUD/面板上的曲名来回闪烁。
 */
export function useBeatmapDownloadProgress(): BeatmapDownloadProgress | null {
  const [download, setDownload] = useState<BeatmapDownloadProgress | null>(null);
  useEffect(() => {
    const progressDisplay = createDownloadProgressDisplay();
    const pending = desktopApi.onBeatmapDownloadProgress((progress) => {
      setDownload(
        progress.phase === "finished" || progress.phase === "cancelled" ? null : progressDisplay.update(progress),
      );
    });
    return () => {
      void pending.then((unlisten) => unlisten());
    };
  }, []);
  return download;
}
