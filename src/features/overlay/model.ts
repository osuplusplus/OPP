import type { AppSettings, BeatmapDownloadProvider } from "../../shared/types/osu";

export type OverlayMode = "hud" | "panel";

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
