import { useQuery } from "@tanstack/react-query";
import { desktopApi, isTauri } from "../../shared/lib/tauri";
import { defaultSimilarityPreferences } from "../similar-beatmaps/defaults";

export const settingsQueryKey = ["settings"] as const;

export function useSettings() {
  return useQuery({
    queryKey: settingsQueryKey,
    queryFn: () => isTauri() ? desktopApi.getSettings() : Promise.resolve({
      onboarding_version: 0,
      page_onboarding_versions: {},
      ignored_update_version: null,
      reduce_motion: false,
      similarity_index_directory: null,
      beatmap_download_directory: null,
      default_beatmap_download_provider: "sayobot" as const,
      include_video_in_beatmap_downloads: true,
      open_downloaded_beatmaps_after_download: false,
      replay_export_directory: null,
      danser_executable_path: null,
      auto_export_new_replays_with_danser: false,
      danser_render_preferences: {
        settings_profile: "default", skin: "", skip: true, quickstart: false,
        start: null, end: null, speed: 1, pitch: 1, offset: 0, mods: "", mods2: "",
        cs: null, ar: null, od: null, hp: null, no_db_check: true,
        no_update_check: true, debug: false, settings_patch: "",
        frame_width: 1920, frame_height: 1080, fps: 60, encoder: "libx264" as const,
        quality: 14, motion_blur: false, motion_blur_oversample: 16,
      },
      tosu_executable_path: null,
      tosu_api_base_url: "http://127.0.0.1:24050",
      launch_tosu_with_game: false,
      tosu_lyrics_executable_path: null,
      launch_tosu_lyrics_with_tosu: true,
      theme_primary: "cyan" as const,
      theme_secondary: "pink" as const,
      theme_mode: "dark" as const,
      launch_tosu_on_game_detect: false,
      game_session_analysis_on_detect: true,
      preview_volume: 65,
      cache_limit_mb: 512,
      key_bindings: {
        open_local_maps: "Alt+1",
        open_trainer: "Alt+2",
        open_settings: "Alt+,",
      },
      similarity_preferences: defaultSimilarityPreferences,
    }),
    staleTime: Infinity,
    retry: false,
  });
}
