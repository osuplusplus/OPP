import { useEffect, useState } from "react";
import { desktopApi } from "../../shared/lib/tauri";
import type {
  AppSettings,
  BeatmapDownloadProgress,
  TosuLiveSnapshot,
} from "../../shared/types/osu";
import { useMpStatus } from "./mpWatcher";
import type { OverlayMode } from "./model";
import { HudCard } from "./HudCard";
import { DirectPanel } from "./DirectPanel";

/**
 * 覆盖层根组件：模式由后端热键/命令驱动（overlay-mode-changed），
 * 本地点击仅作同步回执。tosu 快照与下载进度全局共享给 HUD 与面板。
 */
export function OverlayWindow() {
  const [mode, setMode] = useState<OverlayMode>("hud");
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [snapshot, setSnapshot] = useState<TosuLiveSnapshot | null>(null);
  const [download, setDownload] = useState<BeatmapDownloadProgress | null>(null);
  const mpActivity = useMpStatus();

  useEffect(() => {
    let disposed = false;
    void desktopApi
      .overlayGetState()
      .then((state) => {
        if (!disposed && state.visible) setMode(state.mode);
      })
      .catch(() => {});
    void desktopApi
      .getSettings()
      .then((value) => {
        if (!disposed) setSettings(value);
      })
      .catch(() => {});
    const subscriptions = [
      desktopApi.onOverlayModeChanged((next) => setMode(next)),
      desktopApi.onTosuLiveData((value) => setSnapshot(value)),
      desktopApi.onBeatmapDownloadProgress((progress) => {
        setDownload(
          progress.phase === "finished" || progress.phase === "cancelled" ? null : progress,
        );
      }),
    ];
    return () => {
      disposed = true;
      subscriptions.forEach((pending) => void pending.then((unlisten) => unlisten()));
    };
  }, []);

  const enterMode = (next: OverlayMode) => {
    setMode(next);
    void desktopApi.overlaySetMode(next).catch(() => {});
  };

  return mode === "panel" ? (
    <DirectPanel
      settings={settings}
      snapshot={snapshot}
      mpActivity={mpActivity}
      download={download}
      onClose={() => enterMode("hud")}
    />
  ) : (
    <HudCard
      settings={settings}
      snapshot={snapshot}
      mpActivity={mpActivity}
      download={download}
      onOpen={() => enterMode("panel")}
    />
  );
}
