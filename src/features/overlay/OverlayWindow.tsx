import { useEffect, useState } from "react";
import { desktopApi } from "../../shared/lib/tauri";
import type {
  AppSettings,
  BeatmapDownloadProgress,
  TosuLiveSnapshot,
} from "../../shared/types/osu";
import { useMpStatus, useIrcStatus } from "./mpWatcher";
import { useChat } from "./chatWatcher";
import type { OverlayMode } from "./model";
import { HudCard } from "./HudCard";
import { DirectPanel } from "./DirectPanel";
import { ChatPanel } from "./ChatPanel";

type PanelTab = "search" | "chat";

/**
 * 覆盖层根组件：Steam 式交互——平时完全隐藏（hidden），全局热键唤出 Direct
 * 面板，再按热键或 Esc 收起回原状态；HUD 常驻状态卡为可选设置。模式由
 * 后端热键/命令驱动（overlay-mode-changed），本地点击仅作同步回执。tosu
 * 快照、下载进度与 IRC 状态全局共享给 HUD 与面板。
 */
export function OverlayWindow() {
  const [mode, setMode] = useState<OverlayMode>("hidden");
  const [tab, setTab] = useState<PanelTab>("search");
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [snapshot, setSnapshot] = useState<TosuLiveSnapshot | null>(null);
  const [download, setDownload] = useState<BeatmapDownloadProgress | null>(null);
  const mpActivity = useMpStatus();
  const irc = useIrcStatus();
  const chat = useChat(mode === "panel" && tab === "chat");

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

  // 面板内按 Esc 收起（Steam 式）；后端会广播模式事件同步回 UI。
  useEffect(() => {
    if (mode !== "panel") return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        void desktopApi.overlayClosePanel().catch(() => {});
      }
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [mode]);

  const enterMode = (next: OverlayMode) => {
    setMode(next);
    void desktopApi.overlaySetMode(next).catch(() => {});
  };

  const openChat = () => {
    setTab("chat");
    chat.markRead();
    enterMode("panel");
  };

  const closePanel = () => {
    void desktopApi.overlayClosePanel().catch(() => {});
  };

  const hotkey = settings?.overlay_hotkey_toggle ?? "Control+Shift+O";

  if (mode === "hidden") {
    return null;
  }

  if (mode !== "panel") {
    return (
      <HudCard
        settings={settings}
        snapshot={snapshot}
        mpActivity={mpActivity}
        download={download}
        chatUnread={chat.unread}
        irc={irc}
        onOpen={() => enterMode("panel")}
        onOpenChat={openChat}
      />
    );
  }

  return (
    <div className="panel">
      <header className="panel-head" data-tauri-drag-region>
        <span className="panel-brand">direct</span>
        <div className="panel-tabs">
          <button
            type="button"
            className={`panel-tab${tab === "search" ? " active" : ""}`}
            onClick={() => setTab("search")}
          >
            谱面
          </button>
          <button
            type="button"
            className={`panel-tab${tab === "chat" ? " active" : ""}`}
            onClick={() => {
              setTab("chat");
              chat.markRead();
            }}
          >
            聊天
            {chat.unread > 0 && tab !== "chat" ? ` · ${chat.unread}` : ""}
          </button>
        </div>
        <span className="panel-hotkey">{hotkey} / Esc 收起</span>
        <button
          type="button"
          className="panel-close"
          onClick={closePanel}
          aria-label="收起面板"
        >
          ✕
        </button>
      </header>
      {tab === "search" ? (
        <DirectPanel
          settings={settings}
          snapshot={snapshot}
          mpActivity={mpActivity}
          download={download}
        />
      ) : (
        <ChatPanel settings={settings} entries={chat.entries} irc={irc} />
      )}
    </div>
  );
}
