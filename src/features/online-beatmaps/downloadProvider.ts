import type { AppSettings, BeatmapDownloadProvider } from "../../shared/types/osu";

export function resolveDefaultDownloadProvider(
  settings: Pick<AppSettings, "default_beatmap_download_provider"> | null | undefined,
): BeatmapDownloadProvider {
  const provider = settings?.default_beatmap_download_provider;
  return provider === "osudirect" ||
    provider === "nekoha" ||
    provider === "hinai" ||
    provider === "catboy" ||
    provider === "nerinyan"
    ? provider
    : "sayobot";
}
