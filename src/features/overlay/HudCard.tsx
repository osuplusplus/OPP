import type {
  AppSettings,
  BeatmapDownloadProgress,
  OverlayMpActivity,
  TosuLiveSnapshot,
} from "../../shared/types/osu";
import { displayTitle } from "./model";

const stateLabels: Record<string, string> = {
  Menu: "主菜单",
  SongSelect: "选图",
  Playing: "游戏中",
  ResultScreen: "结算",
  Editor: "编辑器",
  MultiplayerLobby: "多人大厅",
  MultiplayerGameplay: "多人游戏",
  Lobby: "多人大厅",
};

function describeState(state: string | null | undefined): string {
  if (!state) return "无数据";
  return stateLabels[state] ?? state;
}

function describeMpActivity(activity: OverlayMpActivity): { text: string; tone: string } | null {
  switch (activity.phase) {
    case "idle":
      return null;
    case "checking":
      return { text: `检查本地谱面：${activity.title}`, tone: "info" };
    case "missing":
      return { text: `缺少谱面：${activity.title}`, tone: "warn" };
    case "downloading":
      return { text: `MP 补缺下载：${activity.title}`, tone: "info" };
    case "imported":
      return { text: `已下载并导入：${activity.title}`, tone: "ok" };
    case "present":
      return { text: `已有谱面：${activity.title}`, tone: "ok" };
    case "unresolved":
      return { text: `无法识别谱面（可能未提交）：${activity.title}`, tone: "warn" };
    case "error":
      return { text: activity.message, tone: "warn" };
    default:
      return null;
  }
}

function downloadPercent(progress: BeatmapDownloadProgress): number | null {
  const downloaded = progress.downloaded_bytes ?? 0;
  const total = progress.total_bytes;
  if (total == null || total <= 0) return null;
  return Math.min(100, Math.round((downloaded / total) * 100));
}

export function HudCard({
  settings,
  snapshot,
  mpActivity,
  download,
  onOpen,
}: {
  settings: AppSettings | null;
  snapshot: TosuLiveSnapshot | null;
  mpActivity: OverlayMpActivity;
  download: BeatmapDownloadProgress | null;
  onOpen: () => void;
}) {
  const mp = describeMpActivity(mpActivity);
  const percent = download ? downloadPercent(download) : null;
  const hotkey = settings?.overlay_hotkey_toggle ?? "Control+Shift+O";
  const clickThrough = settings?.overlay_click_through !== false;
  const inMultiplayer = !!snapshot?.state && /multiplayer|lobby/i.test(snapshot.state);

  return (
    <div className="hud-card">
      <div className="hud-head">
        <span className="hud-brand">OPP</span>
        <span className={`hud-state${inMultiplayer ? " is-mp" : ""}`}>
          {describeState(snapshot?.state)}
        </span>
      </div>
      <div className="hud-map" title={displayTitle(snapshot?.artist, snapshot?.title)}>
        <span className="hud-map-title">{displayTitle(snapshot?.artist, snapshot?.title)}</span>
        {snapshot?.difficulty ? <span className="hud-map-diff">{snapshot.difficulty}</span> : null}
      </div>
      {download ? (
        <div className="hud-download">
          <div className="hud-download-row">
            <span className="hud-download-title">{download.current_title ?? "下载中"}</span>
            <span className="hud-download-pct">{percent != null ? `${percent}%` : `${download.processed}/${download.total}`}</span>
          </div>
          <div className="hud-progress">
            <div
              className="hud-progress-fill"
              style={percent != null ? { width: `${percent}%` } : { width: "100%", opacity: 0.35 }}
            />
          </div>
        </div>
      ) : mp ? (
        <div className={`hud-mp hud-mp-${mp.tone}`}>{mp.text}</div>
      ) : (
        <div className="hud-mp hud-mp-idle">tosu 待机 · 等待游戏数据</div>
      )}
      <div className="hud-foot">
        <span className="hud-hotkey">{hotkey} 打开面板</span>
        {clickThrough ? null : (
          <button type="button" className="hud-open" onClick={onOpen}>
            面板
          </button>
        )}
      </div>
    </div>
  );
}
