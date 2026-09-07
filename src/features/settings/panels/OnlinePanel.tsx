import { FolderOpen, Volume2 } from "lucide-react";
import { Card, SectionTitle, Button, Toggle } from "../../../shared/components/ui";
import type { AppSettings, BeatmapDownloadProvider } from "../../../shared/types/osu";
import { desktopApi } from "../../../shared/lib/tauri";

interface OnlinePanelProps {
  settings: AppSettings;
  save: (settings: AppSettings) => Promise<void>;
  busy: boolean;
}

export function OnlinePanel({ settings, save, busy }: OnlinePanelProps) {
  const chooseDownloadDirectory = async () => {
    const selected = await desktopApi.chooseLocalDirectory(
      settings.beatmap_download_directory
    );
    if (selected) await save({ ...settings, beatmap_download_directory: selected });
  };

  return (
    <div className="space-y-5">
      <div>
        <h2 className="text-2xl font-bold text-white">在线谱面</h2>
        <p className="mt-1 text-sm text-slate-400">
          设置在线谱面下载源、保存位置和音频试听。
        </p>
      </div>

      <Card className="p-6">
        <SectionTitle
          title="谱面下载"
          description="设置在线谱面和相似谱面快捷下载使用的默认镜像与保存位置。"
        />

        <label className="mt-5 block">
          <span className="mb-2 block text-sm font-medium text-slate-300">
            默认下载源
          </span>
          <select
            className="w-full rounded-xl border border-white/[0.1] bg-[#0b101b] px-3 py-3 text-sm text-slate-200 outline-none focus:border-cyan-300/45"
            disabled={busy}
            onChange={(event) =>
              void save({
                ...settings,
                default_beatmap_download_provider: event.target
                  .value as BeatmapDownloadProvider,
              })
            }
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
            <Button
              onClick={() => void chooseDownloadDirectory()}
              size="sm"
              variant="secondary"
            >
              <FolderOpen className="size-4" />
              {settings.beatmap_download_directory ? "修改位置" : "选择位置"}
            </Button>
            {settings.beatmap_download_directory ? (
              <Button
                onClick={() =>
                  void save({ ...settings, beatmap_download_directory: null })
                }
                size="sm"
                variant="ghost"
              >
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
            onChange={(value: boolean) =>
              void save({ ...settings, include_video_in_beatmap_downloads: value })
            }
          />
        </div>

        <div className="mt-3">
          <Toggle
            checked={settings.open_downloaded_beatmaps_after_download}
            description="开启后，下载成功的谱面会直接在文件管理器中打开，方便你立即导入 osu! 或查看文件。"
            label="下载后打开文件位置"
            onChange={(value: boolean) =>
              void save({
                ...settings,
                open_downloaded_beatmaps_after_download: value,
              })
            }
          />
        </div>
      </Card>

      <Card className="p-6">
        <SectionTitle
          title="试听"
          description="适用于在线谱面和相似谱面结果的音频试听。"
        />

        <label className="mt-5 flex items-center gap-3 rounded-xl border border-white/[0.1] bg-white/[0.035] p-4">
          <Volume2 className="size-5 text-[var(--theme-primary)]" />
          <span className="flex-1 font-semibold text-slate-100">试听音量</span>
          <span className="w-10 text-right font-mono text-sm text-slate-300">
            {settings.preview_volume}%
          </span>
          <input
            aria-label="试听音量"
            className="w-36 accent-[var(--theme-primary)]"
            max="100"
            min="0"
            onChange={(event) =>
              void save({
                ...settings,
                preview_volume: Number(event.target.value),
              })
            }
            type="range"
            value={settings.preview_volume}
          />
        </label>
      </Card>
    </div>
  );
}
