import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { desktopApi } from "../../shared/lib/tauri";
import type { AppSettings } from "../../shared/types/osu";
import { defaultSimilarityPreferences } from "../similar-beatmaps/defaults";
import { useSettings, settingsQueryKey } from "./api";
import { SettingsLayout, type SettingsCategory } from "./SettingsLayout";
import { GeneralPanel } from "./panels/GeneralPanel";
import { LocalDataPanel } from "./panels/LocalDataPanel";
import { OverlayPanel } from "./panels/OverlayPanel";
import { LogsPanel } from "./panels/LogsPanel";
import { AccountPanel } from "./panels/AccountPanel";
import { AppearancePanel } from "./panels/AppearancePanel";
import { OnlinePanel } from "./panels/OnlinePanel";
import { DirectoriesPanel } from "./panels/DirectoriesPanel";
import { ReplayPanel } from "./panels/ReplayPanel";
import { ToolsCachePanel } from "./panels/ToolsCachePanel";
import { AboutPanel } from "./panels/AboutPanel";

const categoryMeta: Record<SettingsCategory, { title: string; description: string }> = {
  general: { title: "常规", description: "设置游戏模式和客户端，修改后立即生效并自动保存。" },
  logs: { title: "日志与诊断", description: "查看运行日志并排查 OPP 问题。" },
  account: { title: "账户", description: "管理 osu! 账户连接和认证状态。" },
  appearance: { title: "外观", description: "自定义界面主题和色彩。" },
  online: { title: "在线谱面", description: "设置谱面下载、保存位置和音频试听。" },
  directories: { title: "游戏目录", description: "管理 Stable 和 lazer 的本地资源目录。" },
  "local-data": { title: "本地数据", description: "管理数据库、谱面与皮肤索引、扫描更新和在线已有标记。" },
  replay: { title: "回放渲染", description: "配置 Danser、FFmpeg 和回放导出。" },
  overlay: { title: "游戏覆盖", description: "游戏内透明覆盖层：HUD 状态、Direct 面板与多人自动补缺。" },
  tools: { title: "工具与缓存", description: "清理运行缓存并管理本地缩略图空间。" },
  about: { title: "关于", description: "查看版本、更新和社区信息。" },
};

const base: AppSettings = {
  show_local_beatmap_presence: true,
  local_beatmap_presence_scope: "all",
  onboarding_version: 0,
  page_onboarding_versions: {},
  ignored_update_version: null,
  reduce_motion: false,
  similarity_index_directory: null,
  mania_similarity_index_directory: null,
  beatmap_download_directory: null,
  default_beatmap_download_provider: "sayobot",
  include_video_in_beatmap_downloads: true,
  open_downloaded_beatmaps_after_download: false,
  replay_export_directory: null,
  danser_executable_path: null,
  ffmpeg_executable_path: null,
  auto_export_new_replays_with_danser: false,
  danser_render_preferences: {
    settings_profile: "default", skin: "", skip: true, quickstart: false,
    start: null, end: null, speed: 1, pitch: 1, offset: 0, mods: "", mods2: "",
    cs: null, ar: null, od: null, hp: null, no_db_check: true,
    no_update_check: true, debug: false, settings_patch: "",
    frame_width: 1920, frame_height: 1080, fps: 60, encoder: "libx264",
    quality: 14, motion_blur: false, motion_blur_oversample: 16,
  },
  tosu_executable_path: null,
  tosu_api_base_url: "http://127.0.0.1:24050",
  launch_tosu_with_game: false,
  tosu_lyrics_executable_path: null,
  launch_tosu_lyrics_with_tosu: true,
  theme_primary: "cyan",
  theme_secondary: "cyan",
  theme_mode: "dark",
  launch_tosu_on_game_detect: false,
  obs_websocket_url: "ws://127.0.0.1:4455",
  obs_selected_scene: null,
  launch_tosu_on_obs_detect: false,
  suppress_tosu_launch_prompt: false,
  game_session_analysis_on_detect: true,
  preview_volume: 65,
  cache_limit_mb: 512,
  similarity_preferences: defaultSimilarityPreferences,
  overlay_enabled: false,
  overlay_show_with_game: true,
  overlay_auto_hide_on_game_exit: true,
  overlay_corner: "TopRight",
  overlay_hotkey_toggle: "Control+Shift+O",
  overlay_click_through: true,
  overlay_mp_auto_download: true,
  overlay_start_tosu_with_game: true,
  overlay_ingame_enabled: false,
  overlay_irc_username: null,
  translation_provider: "google_free",
  translation_enabled: true,
  translation_target_lang: "zh",
  translation_outgoing_target_lang: "en",
  translation_api_base_url: null,
  translation_api_model: null,
};

export function SettingsPage() {
  const stored = useSettings();
  const queryClient = useQueryClient();
  const [busy, setBusy] = useState(false);
  const [activeCategory, setActiveCategory] = useState<SettingsCategory>("general");
  const settings: AppSettings = {
    ...base,
    ...stored.data,
  };

  const save = async (next: AppSettings) => {
    setBusy(true);
    try {
      const saved = await desktopApi.updateSettings(next);
      queryClient.setQueryData(settingsQueryKey, saved);
      window.dispatchEvent(new CustomEvent("opp:settings-updated", { detail: saved }));
    } finally {
      setBusy(false);
    }
  };

  const panelClass = (...categories: SettingsCategory[]) => categories.includes(activeCategory) ? "p-6" : "hidden";
  const activeMeta = categoryMeta[activeCategory];

  return (
    <SettingsLayout activeCategory={activeCategory} onCategoryChange={setActiveCategory}>
      <div className="space-y-5">
        <div>
          <h2 className="text-2xl font-bold text-white">{activeMeta.title}</h2>
          <p className="mt-1 text-sm text-slate-400">{activeMeta.description}</p>
        </div>
        {activeCategory === "general" ? <GeneralPanel onConfigure={() => setActiveCategory("directories")} /> : null}
        <LogsPanel className={panelClass("logs")} />
        <AccountPanel className={panelClass("account")} />
        <AppearancePanel className={panelClass("appearance")} settings={settings} save={save} />
        <OnlinePanel className={panelClass("online")} settings={settings} save={save} busy={busy} />
        <DirectoriesPanel className={panelClass("directories")} />
        {activeCategory === "local-data" ? <LocalDataPanel settings={settings} save={save} busy={busy} onConfigure={() => setActiveCategory("directories")} /> : null}
        {activeCategory === "overlay" ? <OverlayPanel settings={settings} save={save} busy={busy} /> : null}
        <ReplayPanel className={panelClass("replay")} settings={settings} save={save} />
        <ToolsCachePanel className={panelClass("tools")} settings={settings} save={save} busy={busy} />
        <AboutPanel className={panelClass("about")} />
      </div>
    </SettingsLayout>
  );
}
