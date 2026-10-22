import { useEffect, useState } from "react";
import { Film, FolderOpen, RefreshCw, RotateCcw } from "lucide-react";
import { Badge, Button, Card, SectionTitle } from "../../../shared/components/ui";
import { desktopApi } from "../../../shared/lib/tauri";
import type { AppSettings, DanserStatus, FfmpegStatusInfo } from "../../../shared/types/osu";
import { Toggle } from "./Toggle";

export function ReplayPanel({
  className,
  settings,
  save,
}: {
  className?: string;
  settings: AppSettings;
  save: (next: AppSettings) => Promise<void>;
}) {
  const [danserStatus, setDanserStatus] = useState<DanserStatus | null>(null);
  const [danserBusy, setDanserBusy] = useState(false);
  const [ffmpegStatus, setFfmpegStatus] = useState<FfmpegStatusInfo | null>(null);
  const [ffmpegBusy, setFfmpegBusy] = useState(false);

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

  return (
    <Card className={className}>
      <div className="flex items-start justify-between gap-4">
        <SectionTitle title="回放渲染" description="配置本地 Danser、统一视频导出位置和游戏结束后的新回放处理方式。" />
        <Badge tone={danserStatus?.available && danserStatus.ffmpeg_available ? "success" : "warning"}>
          {danserStatus?.available && danserStatus.ffmpeg_available ? "已就绪" : "需配置"}
        </Badge>
      </div>
      <div className="mt-5 space-y-3">
        <div className="rounded-xl border border-white/[.1] bg-white/[.035] p-4">
          <div className="flex items-start gap-3"><Film className="mt-0.5 size-5 text-[var(--theme-primary)]" /><div className="min-w-0 flex-1"><p className="font-semibold text-slate-100">Danser CLI</p><p className="mt-1 break-all text-xs text-slate-400">{danserStatus?.executable_path ?? (navigator.userAgent.includes("Windows") ? "未检测到 danser-cli.exe" : "未在 PATH 中找到 danser")}</p><p className="mt-1 text-xs text-slate-500">{danserStatus?.message ?? "可自动检测 PATH，或手动选择便携版程序。"}</p></div></div>
          <div className="mt-3 flex flex-wrap gap-2"><Button loading={danserBusy} onClick={() => void chooseDanser()} size="sm" variant="secondary"><FolderOpen className="size-4" />选择程序</Button><Button disabled={danserBusy} onClick={() => void save({ ...settings, danser_executable_path: null }).then(refreshDanser)} size="sm" variant="ghost"><RotateCcw className="size-4" />自动检测</Button><Button disabled={danserBusy} onClick={() => void refreshDanser()} size="sm" variant="ghost"><RefreshCw className="size-4" />刷新状态</Button></div>
        </div>
        <div className="rounded-xl border border-white/[.1] bg-white/[.035] p-4">
          <div className="flex items-start justify-between gap-3">
            <div className="flex items-start gap-3"><Film className="mt-0.5 size-5 text-[var(--theme-primary)]" /><div className="min-w-0 flex-1"><p className="font-semibold text-slate-100">FFmpeg</p><p className="mt-1 break-all text-xs text-slate-400">{ffmpegStatus?.path ?? (navigator.userAgent.includes("Windows") ? "未检测到 ffmpeg.exe" : "未在 PATH 中找到 ffmpeg")}</p><p className="mt-1 text-xs text-slate-500">{ffmpegStatus?.version ?? "实时预览导出与本地渲染使用;可自动检测 PATH 或 danser 发行包,也可手动指定。"}</p></div></div>
            {ffmpegStatus?.path ? <Badge tone={ffmpegStatus.source === "manual" ? "cyan" : "success"}>{ffmpegStatus.source === "manual" ? "手动指定" : ffmpegStatus.source === "path" ? "PATH 检测" : "danser 自带"}</Badge> : null}
          </div>
          <div className="mt-3 flex flex-wrap gap-2"><Button loading={ffmpegBusy} onClick={() => void chooseFfmpeg()} size="sm" variant="secondary"><FolderOpen className="size-4" />选择程序</Button><Button disabled={ffmpegBusy} onClick={() => void save({ ...settings, ffmpeg_executable_path: null }).then(refreshFfmpeg)} size="sm" variant="ghost"><RotateCcw className="size-4" />自动检测</Button><Button disabled={ffmpegBusy} onClick={() => void refreshFfmpeg()} size="sm" variant="ghost"><RefreshCw className="size-4" />刷新状态</Button></div>
        </div>
        <div className="rounded-xl border border-white/[.1] bg-white/[.035] p-4"><p className="text-sm font-semibold text-slate-100">统一回放导出目录</p><p className="mt-1 break-all text-xs text-slate-400">{settings.replay_export_directory ?? "尚未选择；首次本地渲染前必须设置"}</p><div className="mt-3"><Button onClick={() => void chooseReplayExportDirectory()} size="sm" variant="secondary"><FolderOpen className="size-4" />{settings.replay_export_directory ? "修改位置" : "选择位置"}</Button></div></div>
        <Toggle checked={Boolean(settings.auto_export_new_replays_with_danser)} description="游戏退出后的总结面板仍会让你确认；开启后，可渲染的新回放会默认全部选中。" label="总结中默认选中新回放" onChange={(value) => void save({ ...settings, auto_export_new_replays_with_danser: value })} />
        <p className="text-xs leading-5 text-amber-200/80">Danser 仅支持 osu!standard；本地导出还需要 Danser 能访问 FFmpeg。</p>
      </div>
    </Card>
  );
}
