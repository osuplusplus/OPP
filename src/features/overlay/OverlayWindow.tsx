import { useEffect, useState } from "react";
import { desktopApi } from "../../shared/lib/tauri";
import type { AppSettings } from "../../shared/types/osu";
import { useMpStatus, useIrcStatus } from "./mpWatcher";
import { useChat } from "./chatWatcher";
import { useBeatmapDownloadProgress, useTosuSnapshot } from "./liveEvents";
import type { OverlayMode } from "./model";
import { HudCard } from "./HudCard";
import { DirectPanel } from "./DirectPanel";
import { ChatPanel } from "./ChatPanel";

type PanelTab = "search" | "chat";

/**
 * HUD 卡片的实时数据壳:tosu 快照与下载进度只在本地订阅,
 * 高频事件的重渲染被限制在 HUD 卡片内部,不触碰根组件。
 */
function HudCardLive({
  settings,
  mpActivity,
  chatUnread,
  irc,
  onOpen,
  onOpenChat,
}: {
  settings: AppSettings | null;
  mpActivity: ReturnType<typeof useMpStatus>;
  chatUnread: number;
  irc: ReturnType<typeof useIrcStatus>;
  onOpen: () => void;
  onOpenChat: () => void;
}) {
  const snapshot = useTosuSnapshot();
  const download = useBeatmapDownloadProgress();
  return (
    <HudCard
      settings={settings}
      snapshot={snapshot}
      mpActivity={mpActivity}
      download={download}
      chatUnread={chatUnread}
      irc={irc}
      onOpen={onOpen}
      onOpenChat={onOpenChat}
    />
  );
}

/**
 * 覆盖层根组件：Steam 式交互——平时完全隐藏（hidden），全局热键唤出 Direct
 * 面板，再按热键或 Esc 收起回原状态；HUD 常驻状态卡为可选设置。模式由
 * 后端热键/命令驱动（overlay-mode-changed），本地点击仅作同步回执。
 * 根组件只持有 mode/tab/settings 等低频状态；tosu 快照、下载进度由
 * HudCardLive / DirectPanel 的叶子组件自行订阅。
 */
export function OverlayWindow() {
  const [mode, setMode] = useState<OverlayMode>("hidden");
  const [tab, setTab] = useState<PanelTab>("search");
  const [settings, setSettings] = useState<AppSettings | null>(null);
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
    const subscriptions = [desktopApi.onOverlayModeChanged((next) => setMode(next))];
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
      <HudCardLive
        settings={settings}
        mpActivity={mpActivity}
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
        <DirectPanel settings={settings} mpActivity={mpActivity} />
      ) : (
        <ChatPanel settings={settings} entries={chat.entries} irc={irc} />
      )}
    </div>
  );
}
