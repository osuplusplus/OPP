import { useEffect, useState } from "react";
import * as Switch from "@radix-ui/react-switch";
import {
  Check,
  ExternalLink,
  Film,
  FolderOpen,
  Gamepad2,
  LogOut,
  RotateCcw,
  RefreshCw,
  Trash2,
  Volume2,
} from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import { useMode } from "../../app/ModeContext";
import { PageHeader } from "../../shared/components/PageHeader";
import {
  Badge,
  Button,
  Card,
  DataLine,
  InfoTip,
  SectionTitle,
} from "../../shared/components/ui";
import { desktopApi } from "../../shared/lib/tauri";
import type {
  AppSettings,
  BeatmapDownloadProvider,
  OsuClient,
  Ruleset,
  ThemeColor,
  DanserStatus,
  FfmpegStatusInfo,
} from "../../shared/types/osu";
import { authQueryKey, useAuthStatus } from "../auth/api";
import { localSourcesKey, useLocalSources } from "../local-analysis/api";
import { useSettings, settingsQueryKey } from "./api";
import { defaultSimilarityPreferences } from "../similar-beatmaps/defaults";
import { START_ONBOARDING_EVENT } from "../../shared/lib/onboardingEvents";
import {
  COMMUNITY_GROUP_MESSAGE,
  COMMUNITY_GROUP_NUMBER,
} from "../../shared/constants/community";
import { requestManualUpdateCheck } from "../updates/events";

const colors: Array<[ThemeColor, string, string]> = [
  ["cyan", "青色", "#67e8f9"],
  ["blue", "蓝色", "#60a5fa"],
  ["violet", "紫罗兰", "#a78bfa"],
  ["pink", "粉色", "#f472b6"],
  ["orange", "橙色", "#fb923c"],
  ["green", "绿色", "#4ade80"],
];

const modes: Array<[Ruleset, string]> = [
  ["osu", "osu!"],
  ["taiko", "Taiko"],
  ["fruits", "Catch"],
  ["mania", "Mania"],
];

const clients: Array<[OsuClient, string]> = [
  ["stable", "osu! Stable"],
  ["lazer", "osu!lazer"],
];

const base: AppSettings = {
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
};

function Toggle({
  label,
  description,
  checked,
  onChange,
}: {
  label: string;
  description: string;
  checked: boolean;
  onChange: (value: boolean) => void;
}) {
  return (
    <div className="flex items-center justify-between gap-4 rounded-xl border border-white/[0.1] bg-white/[0.035] p-4">
      <div className="flex items-center gap-2">
        <p className="font-semibold text-slate-100">{label}</p>
        <InfoTip text={description} />
      </div>
      <Switch.Root
        checked={checked}
        className="relative h-6 w-11 shrink-0 rounded-full bg-slate-500 data-[state=checked]:bg-[var(--theme-primary)]"
        onCheckedChange={onChange}
      >
        <Switch.Thumb className="block size-5 translate-x-0.5 rounded-full bg-white transition-transform data-[state=checked]:translate-x-5" />
      </Switch.Root>
    </div>
  );
}

export function SettingsPage() {
  const stored = useSettings();
  const auth = useAuthStatus();
  const sources = useLocalSources();
  const { ruleset, setRuleset } = useMode();
  const queryClient = useQueryClient();
  const [busy, setBusy] = useState(false);
  const [accountBusy, setAccountBusy] = useState<"logout" | "reauth" | null>(null);
  const [accountError, setAccountError] = useState<string | null>(null);
  const [sourceBusy, setSourceBusy] = useState<OsuClient | null>(null);
  const [updateBusy, setUpdateBusy] = useState(false);
  const [danserStatus, setDanserStatus] = useState<DanserStatus | null>(null);
  const [danserBusy, setDanserBusy] = useState(false);
  const [ffmpegStatus, setFfmpegStatus] = useState<FfmpegStatusInfo | null>(null);
  const [ffmpegBusy, setFfmpegBusy] = useState(false);
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

  const refreshDanser = async () => {
    try { setDanserStatus(await desktopApi.getDanserStatus()); } catch { setDanserStatus(null); }
  };

  useEffect(() => {
    let cancelled = false;
    const idleWindow = window as Window & {
      requestIdleCallback?: (callback: () => void, options?: { timeout: number }) => number;
      cancelIdleCallback?: (handle: number) => void;
    };
    const useIdle = typeof idleWindow.requestIdleCallback === "function";
    const schedule = useIdle
      ? idleWindow.requestIdleCallback!(() => { if (!cancelled) void refreshDanser(); }, { timeout: 1200 })
      : window.setTimeout(() => { if (!cancelled) void refreshDanser(); }, 350);
    return () => {
      cancelled = true;
      if (useIdle) idleWindow.cancelIdleCallback?.(schedule);
      else window.clearTimeout(schedule);
    };
  }, [settings.danser_executable_path]);

  const chooseDanser = async () => {
    setDanserBusy(true);
    try {
      const selected = await desktopApi.chooseDanserExecutable(settings.danser_executable_path);
      if (selected) await save({ ...settings, danser_executable_path: selected });
      await refreshDanser();
    } finally { setDanserBusy(false); }
  };

  const refreshFfmpeg = async () => {
    try { setFfmpegStatus(await desktopApi.liveRenderGetFfmpegStatus()); } catch { setFfmpegStatus(null); }
  };

  useEffect(() => {
    let cancelled = false;
    const idleWindow = window as Window & {
      requestIdleCallback?: (callback: () => void, options?: { timeout: number }) => number;
      cancelIdleCallback?: (handle: number) => void;
    };
    const useIdle = typeof idleWindow.requestIdleCallback === "function";
    const schedule = useIdle
      ? idleWindow.requestIdleCallback!(() => { if (!cancelled) void refreshFfmpeg(); }, { timeout: 1500 })
      : window.setTimeout(() => { if (!cancelled) void refreshFfmpeg(); }, 500);
    return () => {
      cancelled = true;
      if (useIdle) idleWindow.cancelIdleCallback?.(schedule);
      else window.clearTimeout(schedule);
    };
  }, [settings.ffmpeg_executable_path, settings.danser_executable_path]);

  const chooseFfmpeg = async () => {
    setFfmpegBusy(true);
    try {
      const selected = await desktopApi.chooseFfmpegExecutable(settings.ffmpeg_executable_path);
      if (selected) await save({ ...settings, ffmpeg_executable_path: selected });
      await refreshFfmpeg();
    } finally { setFfmpegBusy(false); }
  };

  const chooseReplayExportDirectory = async () => {
    const selected = await desktopApi.chooseLocalDirectory(settings.replay_export_directory);
    if (selected) await save({ ...settings, replay_export_directory: selected });
  };

  const chooseSource = async (client: OsuClient) => {
    setSourceBusy(client);
    try {
      const source = sources.data?.find((item) => item.client === client);
      const selected = await desktopApi.chooseLocalDirectory(
        source?.configured_path ?? source?.install_root ?? source?.data_root,
      );
      if (!selected) return;
      await desktopApi.setLocalSource(client, selected);
      await queryClient.invalidateQueries({ queryKey: localSourcesKey });
    } finally {
      setSourceBusy(null);
    }
  };

  const resetSource = async (client: OsuClient) => {
    setSourceBusy(client);
    try {
      await desktopApi.resetLocalSource(client);
      await queryClient.invalidateQueries({ queryKey: localSourcesKey });
    } finally {
      setSourceBusy(null);
    }
  };

  const chooseDownloadDirectory = async () => {
    const selected = await desktopApi.chooseBeatmapDownloadDirectory(
      settings.beatmap_download_directory,
    );
    if (selected) await save({ ...settings, beatmap_download_directory: selected });
  };

  const refreshAccount = async () => {
    await queryClient.invalidateQueries({ queryKey: authQueryKey });
    await queryClient.invalidateQueries({ queryKey: ["own-profile"] });
    await queryClient.invalidateQueries({ queryKey: ["scores"] });
  };

  const logout = async () => {
    setAccountBusy("logout");
    setAccountError(null);
    try {
      await desktopApi.disconnectOsu(true);
      await refreshAccount();
    } catch (error) {
      setAccountError((error as { message?: string }).message ?? String(error));
    } finally {
      setAccountBusy(null);
    }
  };

  const reauthenticate = async () => {
    setAccountBusy("reauth");
    setAccountError(null);
    try {
      await desktopApi.disconnectOsu(false);
      await refreshAccount();
      const pending = await desktopApi.beginOAuthLogin();
      await desktopApi.openExternal(pending.authorization_url);
    } catch (error) {
      setAccountError((error as { message?: string }).message ?? String(error));
    } finally {
      setAccountBusy(null);
    }
  };

  const checkForUpdates = () => {
    setUpdateBusy(true);
    requestManualUpdateCheck(() => setUpdateBusy(false));
  };

  const palette = () => (
    <div>
      <p className="mb-3 text-base font-bold text-slate-100">主题色</p>
      <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
        {colors.map(([value, label, color]) => {
          const selected = settings.theme_primary === value;
          return (
            <button
              aria-pressed={selected}
              className={`flex items-center gap-2 rounded-xl border px-3 py-2.5 text-sm font-semibold transition ${selected ? "border-[var(--theme-primary)] bg-[var(--theme-primary-muted)] text-[var(--theme-primary-light)] shadow-[0_0_0_2px_var(--theme-primary-soft)]" : "border-white/10 text-slate-200 hover:bg-white/[0.06]"}`}
              key={value}
              onClick={() => void save({ ...settings, theme_primary: value, theme_secondary: value })}
              type="button"
            >
              <span className="size-4 rounded-full" style={{ background: color }} />
              {label}
              {selected ? <Check className="ml-auto size-4" /> : null}
            </button>
          );
        })}
      </div>
    </div>
  );

  const lightTheme = settings.theme_mode === "light";

  return (
    <>
      <PageHeader
        eyebrow="Application"
        title="设置"
        description="在这里管理主题、游戏来源和常用偏好。"
      />
      <div className="grid gap-5 xl:grid-cols-2">
        <div className="space-y-5">
          <Card className="p-6">
            <div className="flex justify-between">
              <SectionTitle title="账户" />
              <Badge tone={auth.data?.connected ? "success" : "warning"}>
                {auth.data?.connected ? "已连接" : "未连接"}
              </Badge>
            </div>
            <div className="mt-4">
              <DataLine label="账户" value={auth.data?.username ?? "—"} />
              <DataLine label="用户 ID" value={auth.data?.user_id ?? "—"} />
            </div>
            <div className="mt-4 flex flex-wrap gap-2 border-t border-[var(--line-subtle)] pt-4">
              <Button disabled={accountBusy !== null} loading={accountBusy === "reauth"} onClick={() => void reauthenticate()} size="sm" variant="secondary">
                <RotateCcw className="size-4" />重新认证
              </Button>
              <Button disabled={!auth.data?.connected || accountBusy !== null} loading={accountBusy === "logout"} onClick={() => void logout()} size="sm" variant="ghost">
                <LogOut className="size-4" />退出账号
              </Button>
            </div>
            {accountError ? <p className="mt-3 text-xs text-rose-200">{accountError}</p> : null}
          </Card>

          <Card className="p-6">
            <SectionTitle title="主题" />
            <div className="mt-5 flex items-center justify-between rounded-xl border border-white/[0.1] bg-white/[0.035] p-4">
              <div className="flex items-center gap-2">
                <p className="font-semibold text-slate-100">浅色主题</p>
                <InfoTip text="开启后使用浅色界面；关闭时使用默认深色界面。" />
              </div>
              <Switch.Root
                checked={lightTheme}
                className="relative h-6 w-11 shrink-0 rounded-full bg-slate-500 data-[state=checked]:bg-[var(--theme-primary)]"
                onCheckedChange={(value) => void save({ ...settings, theme_mode: value ? "light" : "dark" })}
              >
                <Switch.Thumb className="block size-5 translate-x-0.5 rounded-full bg-white transition-transform data-[state=checked]:translate-x-5" />
              </Switch.Root>
            </div>
            <div className="mt-6">{palette()}</div>
          </Card>

          <Card className="p-6">
            <SectionTitle title="默认游戏模式" description="选择应用打开时优先使用的 osu! 游戏模式。" />
            <div className="mt-5 grid grid-cols-2 gap-2">
              {modes.map(([value, label]) => {
                const selected = ruleset === value;
                return (
                  <button
                    aria-pressed={selected}
                    className={`rounded-xl border px-3 py-3 text-sm font-semibold transition ${selected ? "border-[var(--theme-primary)] bg-[var(--theme-primary-muted)] text-[var(--theme-primary-light)]" : "border-white/10 text-slate-200 hover:bg-white/[0.06]"}`}
                    key={value}
                    onClick={() => setRuleset(value)}
                    type="button"
                  >
                    {label}
                  </button>
                );
              })}
            </div>
          </Card>

          <Card className="p-6">
            <SectionTitle title="试听" description="适用于在线谱面和相似谱面结果的音频试听。" />
            <label className="mt-5 flex items-center gap-3 rounded-xl border border-white/[0.1] bg-white/[0.035] p-4">
              <Volume2 className="size-5 text-[var(--theme-primary)]" />
              <span className="flex-1 font-semibold text-slate-100">试听音量</span>
              <span className="w-10 text-right font-mono text-sm text-slate-300">{settings.preview_volume}%</span>
              <input
                aria-label="试听音量"
                className="w-36 accent-[var(--theme-primary)]"
                max="100"
                min="0"
                onChange={(event) => void save({ ...settings, preview_volume: Number(event.target.value) })}
                type="range"
                value={settings.preview_volume}
              />
            </label>
          </Card>
        </div>

        <div className="space-y-5">
          <Card className="p-6">
            <SectionTitle title="游戏目录" description="为 Stable 和 lazer 分别选择 osu! 安装或数据目录，保存后会重新建立本地资源索引。" />
            <div className="mt-5 space-y-3">
              {clients.map(([client, label]) => {
                const source = sources.data?.find((item) => item.client === client);
                const path = source?.configured_path ?? source?.data_root ?? source?.install_root;
                return (
                  <div className="rounded-xl border border-white/[0.1] bg-white/[0.035] p-4" key={client}>
                    <div className="flex items-start justify-between gap-3">
                      <div className="min-w-0">
                        <p className="font-semibold text-slate-100">{label}</p>
                        <p className="mt-1 truncate text-xs text-slate-400" title={path ?? undefined}>
                          {path ?? "未选择目录，将自动检测"}
                        </p>
                      </div>
                      <Badge tone={source?.valid ? "success" : "warning"}>
                        {source?.valid ? "可用" : "未配置"}
                      </Badge>
                    </div>
                    <div className="mt-3 flex flex-wrap gap-2">
                      <Button loading={sourceBusy === client} onClick={() => void chooseSource(client)} size="sm" variant="secondary">
                        <FolderOpen className="size-4" />选择目录
                      </Button>
                      <Button disabled={sourceBusy !== null} onClick={() => void resetSource(client)} size="sm" variant="ghost">
                        <RotateCcw className="size-4" />自动检测
                      </Button>
                    </div>
                  </div>
                );
              })}
            </div>
          </Card>

          <Card className="p-6">
            <div className="flex items-start justify-between gap-4">
              <SectionTitle title="回放渲染" description="配置本地 Danser、统一视频导出位置和游戏结束后的新回放处理方式。" />
              <Badge tone={danserStatus?.available && danserStatus.ffmpeg_available ? "success" : "warning"}>
                {danserStatus?.available && danserStatus.ffmpeg_available ? "已就绪" : "需配置"}
              </Badge>
            </div>
            <div className="mt-5 space-y-3">
              <div className="rounded-xl border border-white/[0.1] bg-white/[0.035] p-4">
                <div className="flex items-start gap-3"><Film className="mt-0.5 size-5 text-[var(--theme-primary)]" /><div className="min-w-0 flex-1"><p className="font-semibold text-slate-100">Danser CLI</p><p className="mt-1 break-all text-xs text-slate-400">{danserStatus?.executable_path ?? (navigator.userAgent.includes("Windows") ? "未检测到 danser-cli.exe" : "未在 PATH 中找到 danser")}</p><p className="mt-1 text-xs text-slate-500">{danserStatus?.message ?? "可自动检测 PATH，或手动选择便携版程序。"}</p></div></div>
                <div className="mt-3 flex flex-wrap gap-2"><Button loading={danserBusy} onClick={() => void chooseDanser()} size="sm" variant="secondary"><FolderOpen className="size-4" />选择程序</Button><Button disabled={danserBusy} onClick={() => void save({ ...settings, danser_executable_path: null }).then(refreshDanser)} size="sm" variant="ghost"><RotateCcw className="size-4" />自动检测</Button><Button disabled={danserBusy} onClick={() => void refreshDanser()} size="sm" variant="ghost"><RefreshCw className="size-4" />刷新状态</Button></div>
              </div>
              <div className="rounded-xl border border-white/[0.1] bg-white/[0.035] p-4">
                <div className="flex items-start justify-between gap-3">
                  <div className="flex items-start gap-3"><Film className="mt-0.5 size-5 text-[var(--theme-primary)]" /><div className="min-w-0 flex-1"><p className="font-semibold text-slate-100">FFmpeg</p><p className="mt-1 break-all text-xs text-slate-400">{ffmpegStatus?.path ?? (navigator.userAgent.includes("Windows") ? "未检测到 ffmpeg.exe" : "未在 PATH 中找到 ffmpeg")}</p><p className="mt-1 text-xs text-slate-500">{ffmpegStatus?.version ?? "实时预览导出与本地渲染使用;可自动检测 PATH 或 danser 发行包,也可手动指定。"}</p></div></div>
                  {ffmpegStatus?.path ? <Badge tone={ffmpegStatus.source === "manual" ? "cyan" : "success"}>{ffmpegStatus.source === "manual" ? "手动指定" : ffmpegStatus.source === "path" ? "PATH 检测" : "danser 自带"}</Badge> : null}
                </div>
                <div className="mt-3 flex flex-wrap gap-2"><Button loading={ffmpegBusy} onClick={() => void chooseFfmpeg()} size="sm" variant="secondary"><FolderOpen className="size-4" />选择程序</Button><Button disabled={ffmpegBusy} onClick={() => void save({ ...settings, ffmpeg_executable_path: null }).then(refreshFfmpeg)} size="sm" variant="ghost"><RotateCcw className="size-4" />自动检测</Button><Button disabled={ffmpegBusy} onClick={() => void refreshFfmpeg()} size="sm" variant="ghost"><RefreshCw className="size-4" />刷新状态</Button></div>
              </div>
              <div className="rounded-xl border border-white/[0.1] bg-white/[0.035] p-4"><p className="text-sm font-semibold text-slate-100">统一回放导出目录</p><p className="mt-1 break-all text-xs text-slate-400">{settings.replay_export_directory ?? "尚未选择；首次本地渲染前必须设置"}</p><div className="mt-3"><Button onClick={() => void chooseReplayExportDirectory()} size="sm" variant="secondary"><FolderOpen className="size-4" />{settings.replay_export_directory ? "修改位置" : "选择位置"}</Button></div></div>
              <Toggle checked={Boolean(settings.auto_export_new_replays_with_danser)} description="游戏退出后的总结面板仍会让你确认；开启后，可渲染的新回放会默认全部选中。" label="总结中默认选中新回放" onChange={(value) => void save({ ...settings, auto_export_new_replays_with_danser: value })} />
              <p className="text-xs leading-5 text-amber-200/80">Danser 仅支持 osu!standard；本地导出还需要 Danser 能访问 FFmpeg。</p>
            </div>
          </Card>

          <Card className="p-6">
            <SectionTitle title="工具与缓存" />
            <div className="mt-5 space-y-3">
              <Toggle
                checked={settings.similarity_preferences.advanced_enabled}
                description="开启后可调整 osu!standard 的六组固定推荐权重；Mania 使用 Analyzer 分类优先排序与 NM / DT / HT Mod 池。"
                label="相似谱面高级设置"
                onChange={(value) => void save({
                  ...settings,
                  similarity_preferences: {
                    ...settings.similarity_preferences,
                    advanced_enabled: value,
                  },
                })}
              />
              <label className="block rounded-xl border border-white/[0.1] bg-white/[0.035] p-4">
                <span className="block text-sm font-semibold text-slate-100">相似谱面每页数量</span>
                <span className="mt-1 block text-xs leading-5 text-slate-500">控制相似结果列表每一批展示的谱面数量，同时影响“下载本批”。</span>
                <select
                  aria-label="相似谱面每页数量"
                  className="opp-input mt-3 w-36"
                  disabled={busy}
                  onChange={(event) => void save({
                    ...settings,
                    similarity_preferences: {
                      ...settings.similarity_preferences,
                      results_per_page: Number(event.target.value),
                    },
                  })}
                  value={settings.similarity_preferences.results_per_page}
                >
                  {[5, 10, 15, 20].map((count) => <option key={count} value={count}>{count} 张 / 页</option>)}
                </select>
              </label>
              <div className="flex flex-wrap gap-3">
              <Button disabled={busy} onClick={() => void desktopApi.clearProfileCache()}>
                <Trash2 className="size-4" />清除缓存
              </Button>
              </div>
              <label className="block rounded-xl border border-white/[0.1] bg-white/[0.035] p-4">
                <span className="block text-sm font-semibold text-slate-100">本地缩略图缓存上限</span>
                <span className="mt-1 block text-xs leading-5 text-slate-500">超过上限时会自动清理最早的谱面背景缩略图；索引和你的谱面文件不会被删除。</span>
                <div className="mt-3 flex items-center gap-2">
                  <input
                    aria-label="本地缩略图缓存上限（MB）"
                    className="opp-input w-32"
                    disabled={busy}
                    max={10240}
                    min={64}
                    onChange={(event) => void save({ ...settings, cache_limit_mb: Math.max(64, Math.min(10240, Number(event.target.value) || 64)) })}
                    step={64}
                    type="number"
                    value={settings.cache_limit_mb}
                  />
                  <span className="text-sm text-slate-400">MB</span>
                </div>
              </label>
            </div>
          </Card>

          <Card className="p-6">
            <SectionTitle
              title="谱面下载"
              description="设置在线谱面和相似谱面快捷下载使用的默认镜像与保存位置。"
            />
            <label className="mt-5 block">
              <span className="mb-2 block text-sm font-medium text-slate-300">默认下载源</span>
              <select
                className="w-full rounded-xl border border-white/[0.1] bg-[#0b101b] px-3 py-3 text-sm text-slate-200 outline-none focus:border-cyan-300/45"
                disabled={busy}
                onChange={(event) => void save({
                  ...settings,
                  default_beatmap_download_provider: event.target.value as BeatmapDownloadProvider,
                })}
                value={settings.default_beatmap_download_provider}
              >
                <option value="sayobot">小夜（Sayobot，推荐）</option>
                <option value="hinai">Hinai Mirror（多源回退）</option>
                <option value="catboy">Catboy</option>
                <option value="nerinyan">Nerinyan</option>
              </select>
              <span className="mt-2 block text-xs leading-5 text-slate-500">
                下载失败时仍会自动尝试其他可用镜像；下载队列中可以临时切换，不会改动此默认值。
              </span>
            </label>
            <div className="mt-4 rounded-xl border border-white/[0.1] bg-white/[0.035] p-4">
              <p className="text-xs text-slate-500">当前默认位置</p>
              <p className="mt-1 break-all text-sm text-slate-200">
                {settings.beatmap_download_directory ?? "Windows 下载目录"}
              </p>
              <div className="mt-3 flex flex-wrap gap-2">
                <Button onClick={() => void chooseDownloadDirectory()} size="sm" variant="secondary">
                  <FolderOpen className="size-4" />
                  {settings.beatmap_download_directory ? "修改位置" : "选择位置"}
                </Button>
                {settings.beatmap_download_directory ? (
                  <Button onClick={() => void save({ ...settings, beatmap_download_directory: null })} size="sm" variant="ghost">
                    清除默认位置
                  </Button>
                ) : null}
              </div>
            </div>
            <div className="mt-3">
              <Toggle
                checked={settings.include_video_in_beatmap_downloads}
                description="开启后下载完整谱面包（可能包含背景视频，文件更大）；关闭后会向镜像请求不含视频的版本。该偏好适用于在线谱面、相似谱面、收藏夹和 BeatmapHub 补全下载。"
                label="下载谱面时包含视频"
                onChange={(value) => void save({ ...settings, include_video_in_beatmap_downloads: value })}
              />
            </div>
            <div className="mt-3">
              <Toggle
                checked={settings.open_downloaded_beatmaps_after_download}
                description="下载成功后用系统默认程序打开每个新下载的 .osz 文件，osu! 会自动导入这些谱面。此选项同时适用于在线批量下载和相似谱面的快捷下载。"
                label="下载完成后自动打开谱面"
                onChange={(value) => void save({ ...settings, open_downloaded_beatmaps_after_download: value })}
              />
            </div>
          </Card>

          <Card className="p-6">
            <div className="flex items-start justify-between gap-4">
              <div>
                <SectionTitle title="关于" />
                <p className="mt-3 flex items-center gap-2 text-sm text-slate-300">
                  <Gamepad2 className="size-4 text-[var(--theme-primary)]" />
                  OPP v{__APP_VERSION__}
                </p>
                <div className="mt-4 rounded-xl border border-white/[0.1] bg-white/[0.035] px-4 py-3">
                  <p className="text-sm text-slate-300">{COMMUNITY_GROUP_MESSAGE}</p>
                  <p className="mt-1 text-sm text-slate-400">
                    交流群：<span className="select-all font-mono font-semibold text-[var(--theme-primary-light)]">{COMMUNITY_GROUP_NUMBER}</span>
                  </p>
                </div>
              </div>
              <div className="flex flex-wrap justify-end gap-2">
                <Button disabled={updateBusy} loading={updateBusy} onClick={() => void checkForUpdates()} size="sm" variant="secondary">
                  <RefreshCw className="size-4" />检查更新
                </Button>
                <Button onClick={() => window.dispatchEvent(new Event(START_ONBOARDING_EVENT))} size="sm" variant="secondary">
                  <RotateCcw className="size-4" />重新查看新手引导
                </Button>
                <Button onClick={() => void desktopApi.openExternal("https://github.com/osuplusplus/OPP")} size="sm" variant="secondary">
                  <ExternalLink className="size-4" />项目仓库
                </Button>
              </div>
            </div>
          </Card>
        </div>
      </div>
    </>
  );
}
