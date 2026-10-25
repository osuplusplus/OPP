import type { AppSettings, BeatmapDownloadProvider } from "../../shared/types/osu";

/** 覆盖层形态：hidden 完全隐藏（默认）| hud 常驻状态卡 | panel Direct 侧栏。 */
export type OverlayMode = "hidden" | "hud" | "panel";

/** tosu 菜单状态中与多人/大厅相关的取值（不区分大小写，容错匹配）。 */
export function isMultiplayerState(state: string | null | undefined): boolean {
  return !!state && /multiplayer|lobby/i.test(state);
}

export function resolveDownloadProvider(
  settings: AppSettings | null | undefined,
): BeatmapDownloadProvider {
  const provider = settings?.default_beatmap_download_provider;
  return provider === "hinai" || provider === "catboy" || provider === "nerinyan"
    ? provider
    : "sayobot";
}

export function displayTitle(
  artist: string | null | undefined,
  title: string | null | undefined,
): string {
  const trimmedTitle = title?.trim();
  if (!trimmedTitle) return "未知谱面";
  return artist?.trim() ? `${artist.trim()} - ${trimmedTitle}` : trimmedTitle;
}

/** 聊天昵称着色色板：同一昵称哈希到固定颜色，便于在消息流中区分玩家。 */
const CHAT_NICK_PALETTE = [
  "#4fc3f7",
  "#ffb74d",
  "#81c784",
  "#ba68c8",
  "#f06292",
  "#ffd54f",
  "#4db6ac",
  "#7986cb",
] as const;

/** 按昵称稳定着色：乘 31 字符哈希取模色板长度。 */
export function chatNickColor(nick: string): string {
  let hash = 0;
  for (let index = 0; index < nick.length; index++) {
    hash = (hash * 31 + nick.charCodeAt(index)) >>> 0;
  }
  return CHAT_NICK_PALETTE[hash % CHAT_NICK_PALETTE.length]!;
}

/**
 * 难度星数 → 官网风格配色：0-1★ 蓝 → 4★ 青/绿 → 6★ 黄/橙 → 7★ 红，
 * 8★以上转品红/紫（近似 osu!web 的难度色阶）。
 */
export function starColor(stars: number): string {
  const clamped = Math.max(0, Math.min(stars, 9));
  const hue = clamped <= 7 ? 210 - (clamped / 7) * 210 : 330 - (clamped - 7) * 30;
  return `hsl(${Math.round(hue)} 78% 62%)`;
}

/** 谱面集上架状态徽标（官网同款语义与近似配色）。 */
export const BEATMAP_STATUS_META: Record<string, { label: string; color: string }> = {
  ranked: { label: "上架", color: "#3ec3ff" },
  approved: { label: "上架", color: "#3ec3ff" },
  qualified: { label: "过审", color: "#c05fd8" },
  loved: { label: "Loved", color: "#ff66ab" },
  pending: { label: "待定", color: "#e8b23a" },
  wip: { label: "制作中", color: "#e8b23a" },
  graveyard: { label: "坟场", color: "#8f8f8f" },
};

/** 取状态徽标元数据；未知状态回退灰色 "未知"。 */
export function statusMeta(status: string | null | undefined): { label: string; color: string } {
  if (status && BEATMAP_STATUS_META[status]) return BEATMAP_STATUS_META[status]!;
  return { label: "未知", color: "#8f8f8f" };
}

/** 播放/收藏计数缩写：12345 → 1.2万（官网风格紧凑展示）。 */
export function compactCount(value: number | null | undefined): string {
  if (value == null) return "—";
  if (value >= 100_000_000) return `${(value / 100_000_000).toFixed(1)}亿`;
  if (value >= 10_000) return `${(value / 10_000).toFixed(1)}万`;
  return String(value);
}

export interface OverlayIrcStatusLike {
  irc: { state: string; message: string };
  ircConfigured: boolean;
}

/**
 * 聊天页的 IRC 状态提示：未配置/出错给可操作的警示，连接中/未连接给说明，
 * 正常连接返回 null（频道行本身已可见）。
 */
export function ircNotice(
  status: OverlayIrcStatusLike | null,
): { tone: "warn" | "info"; text: string } | null {
  if (!status) return null;
  if (!status.ircConfigured) {
    return {
      tone: "warn",
      text: "IRC 未配置：多人聊天与自动补缺不可用。请到 OPP 主窗口「设置 > 游戏覆盖」填写 IRC 用户名与令牌。",
    };
  }
  if (status.irc.state === "error") {
    const message = status.irc.message.trim();
    return {
      tone: "warn",
      text: message
        ? `IRC 连接失败：${message}。请到「设置 > 游戏覆盖」检查凭据。`
        : "IRC 连接失败，请到「设置 > 游戏覆盖」检查凭据。",
    };
  }
  if (status.irc.state === "connecting") {
    return { tone: "info", text: "IRC 连接中…" };
  }
  if (status.irc.state === "disconnected") {
    return { tone: "info", text: "IRC 未连接：osu! 启动后自动连接。" };
  }
  return null;
}
