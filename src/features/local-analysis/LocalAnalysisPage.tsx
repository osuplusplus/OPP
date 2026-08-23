import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import {
  Ban,
  ChevronDown,
  ChevronUp,
  Database,
  FileMusic,
  FolderOpen,
  HardDrive,
  Palette,
  RefreshCw,
  RotateCcw,
} from "lucide-react";
import { useMode } from "../../app/ModeContext";
import { ErrorPanel } from "../../shared/components/ErrorPanel";
import { PageHeader } from "../../shared/components/PageHeader";
import {
  Badge,
  Button,
  Card,
  EmptyState,
  Skeleton,
} from "../../shared/components/ui";
import { dateTime, fullNumber } from "../../shared/lib/format";
import { desktopApi } from "../../shared/lib/tauri";
import type {
  CommandError,
  LocalLibrarySummary,
  LocalScanProgress,
  LocalSourceStatus,
} from "../../shared/types/osu";
import {
  localSourcesKey,
  localSummaryKey,
  useLocalSources,
  useLocalIndexStatus,
  useLocalSummary,
} from "./api";
import { BeatmapSetPanel } from "./BeatmapSetPanel";
import { BeatmapDetailDrawer } from "./LocalDetailDrawers";
import { SkinPanel } from "./SkinPanel";

export type LocalSection = "maps" | "skins";

const phaseLabels: Record<LocalScanProgress["phase"], string> = {
  discovery: "发现文件",
  indexing: "比对索引",
  beatmaps: "解析谱面",
  difficulty: "计算难度",
  skins: "分析 Skin",
  finalizing: "保存索引",
};

function formatBytes(value?: number | null) {
  if (value === null || value === undefined) return "—";
  if (value < 1024) return `${value} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let size = value / 1024;
  let unit = units[0];
  for (let index = 1; size >= 1024 && index < units.length; index += 1) {
    size /= 1024;
    unit = units[index];
  }
  return `${size.toFixed(size >= 10 ? 1 : 2)} ${unit}`;
}

function SourceBar({
  action,
  onCancel,
  onChoose,
  onReset,
  onScan,
  progress,
  scanning,
  section,
  source,
  summary,
}: {
  action: string | null;
  onCancel: () => void;
  onChoose: () => void;
  onReset: () => void;
  onScan: (force: boolean) => void;
  progress: LocalScanProgress | null;
  scanning: boolean;
  section: LocalSection;
  source: LocalSourceStatus | undefined;
  summary: LocalLibrarySummary | null | undefined;
}) {
  const [expanded, setExpanded] = useState(false);
  if (!source) return <Skeleton className="mb-4 h-20" />;

  const clientLabel = source.client === "stable" ? "Stable" : "Lazer";
  const Icon = section === "maps" ? FileMusic : Palette;
  const primaryCount =
    section === "maps" ? summary?.beatmap_set_count : summary?.skin_count;
  const secondaryCount =
    section === "maps" ? summary?.beatmap_count : summary?.source_file_count;

  return (
    <Card className="mb-4 overflow-hidden">
      <div className="flex min-h-16 items-center gap-4 px-4 py-3">
        <div
          className={`grid size-9 shrink-0 place-items-center rounded-xl border ${
            source.valid
              ? "border-cyan-300/15 bg-cyan-300/10 text-cyan-100"
              : "border-amber-300/15 bg-amber-300/10 text-amber-100"
          }`}
        >
          <Icon className="size-4" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <p className="text-sm font-semibold text-white">{clientLabel}</p>
            <Badge tone={source.valid ? "success" : "warning"}>
              {source.valid ? "已连接" : "需配置"}
            </Badge>
            {source.client === "lazer" ? <Badge tone="success">Realm 索引</Badge> : null}
            {section === "maps" && summary ? (
              <Badge>
                {summary.calculation.engine} {summary.calculation.engine_version}
              </Badge>
            ) : null}
          </div>
          <p className="mt-1 truncate font-mono text-[10px] text-slate-600">
            {source.data_root ?? "尚未解析数据目录"}
          </p>
        </div>

        {summary ? (
          <div className="flex shrink-0 items-center gap-6 border-l border-white/[0.06] pl-5">
            <div>
              <p className="font-mono text-base font-semibold text-white">
                {fullNumber(primaryCount ?? 0)}
              </p>
              <p className="text-[9px] uppercase tracking-wider text-slate-600">
                {section === "maps" ? "Sets" : "Skins"}
              </p>
            </div>
            <div>
              <p className="font-mono text-base font-semibold text-slate-300">
                {fullNumber(secondaryCount ?? 0)}
              </p>
              <p className="text-[9px] uppercase tracking-wider text-slate-600">
                {section === "maps" ? "Maps" : "Files"}
              </p>
            </div>
          </div>
        ) : null}

        {scanning ? (
          <Button onClick={onCancel} size="sm" variant="danger">
            <Ban className="size-3.5" />
            取消
          </Button>
        ) : (
          <Button
            disabled={!source.valid}
            onClick={() => onScan(false)}
            size="sm"
            variant="primary"
          >
            <RefreshCw className="size-3.5" />
            {summary ? "增量刷新" : "扫描"}
          </Button>
        )}
        <Button
          aria-expanded={expanded}
          aria-label="数据源设置"
          onClick={() => setExpanded((value) => !value)}
          size="icon"
          variant="ghost"
        >
          {expanded ? <ChevronUp className="size-4" /> : <ChevronDown className="size-4" />}
        </Button>
      </div>

      {scanning && progress ? (
        <div className="border-t border-white/[0.055] bg-black/10 px-4 py-3">
          <div className="mb-2 flex items-center justify-between text-[10px]">
            <span className="text-slate-400">
              {phaseLabels[progress.phase]}
              {progress.total
                ? ` · ${fullNumber(progress.processed)} / ${fullNumber(progress.total)}`
                : ""}
            </span>
            <span className="font-mono text-cyan-200">{progress.percent.toFixed(1)}%</span>
          </div>
          <div className="h-1 overflow-hidden rounded-full bg-white/[0.06]">
            <div
                className="h-full rounded-full bg-[var(--theme-primary)] transition-[width] duration-150"
              style={{ width: `${progress.percent}%` }}
            />
          </div>
        </div>
      ) : null}

      {expanded ? (
        <div className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-5 border-t border-white/[0.055] bg-black/10 px-4 py-3">
          <div className="grid min-w-0 grid-cols-4 gap-5 text-[10px]">
            <div className="min-w-0">
              <p className="uppercase tracking-wider text-slate-700">数据目录</p>
              <p className="mt-1 truncate font-mono text-slate-400">
                {source.data_root ?? "—"}
              </p>
            </div>
            <div className="min-w-0">
              <p className="uppercase tracking-wider text-slate-700">安装与检测</p>
              <p className="mt-1 truncate text-slate-400">
                {source.mode === "auto" ? "自动检测" : "手动目录"}
                {source.version ? ` · ${source.version}` : ""}
              </p>
            </div>
            <div className="min-w-0">
              <p className="uppercase tracking-wider text-slate-700">最近索引</p>
              <p className="mt-1 truncate text-slate-400">
                {summary ? `${dateTime(summary.scanned_at)} · ${formatBytes(summary.source_bytes)}` : "尚未扫描"}
              </p>
            </div>
            <div className="min-w-0">
              <p className="uppercase tracking-wider text-slate-700">难度与 PP 算法</p>
              <p
                className="mt-1 truncate text-slate-400"
                title={
                  summary
                    ? `${summary.calculation.upstream_repository}@${summary.calculation.upstream_revision}`
                    : undefined
                }
              >
                {summary
                  ? `${summary.calculation.engine} ${summary.calculation.engine_version} · ${summary.calculation.upstream_date}`
                  : "随首次扫描记录"}
              </p>
            </div>
          </div>
          <div className="flex gap-2">
            <Button onClick={onChoose} size="sm">
              <FolderOpen className="size-3.5" />
              选择目录
            </Button>
            {source.mode === "override" ? (
              <Button onClick={onReset} size="sm">
                <RotateCcw className="size-3.5" />
                自动检测
              </Button>
            ) : null}
            {summary ? (
              <Button disabled={scanning || !source.valid} onClick={() => onScan(true)} size="sm">
                强制重建
              </Button>
            ) : null}
          </div>
        </div>
      ) : null}

      {source.validation_errors.length || action ? (
        <div className="border-t border-rose-300/10 bg-rose-300/[0.05] px-4 py-2 text-xs text-rose-200">
          {action ?? source.validation_errors[0]}
        </div>
      ) : null}
    </Card>
  );
}

function LocalAnalysisClientPage({ section }: { section: LocalSection }) {
  const { client, ruleset } = useMode();
  const queryClient = useQueryClient();
  const sourcesQuery = useLocalSources();
  const indexStatusQuery = useLocalIndexStatus();
  const summaryQuery = useLocalSummary(client);
  const source = sourcesQuery.data?.find((item) => item.client === client);
  const summary = summaryQuery.data;
  const [scanning, setScanning] = useState(false);
  const [progress, setProgress] = useState<LocalScanProgress | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [selectedBeatmap, setSelectedBeatmap] = useState<string | null>(null);

  useEffect(() => {
    if (indexStatusQuery.data?.phase === "ready") {
      void queryClient.invalidateQueries({ queryKey: localSummaryKey(client) });
      void queryClient.invalidateQueries({ queryKey: localSourcesKey });
    }
  }, [client, indexStatusQuery.data?.phase, queryClient]);

  useEffect(() => {
    let unlisten: () => void = () => undefined;
    desktopApi
      .onLocalScanProgress((event) => {
        if (event.client === client) setProgress(event);
      })
      .then((dispose) => {
        unlisten = dispose;
      });
    return () => unlisten();
  }, [client]);

  const invalidateLocal = async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: localSourcesKey }),
      queryClient.invalidateQueries({ queryKey: localSummaryKey(client) }),
      queryClient.invalidateQueries({ queryKey: ["local-beatmaps"] }),
      queryClient.invalidateQueries({ queryKey: ["local-beatmap-sets"] }),
      queryClient.invalidateQueries({ queryKey: ["local-beatmap-background"] }),
      queryClient.invalidateQueries({ queryKey: ["local-skins"] }),
      queryClient.invalidateQueries({ queryKey: ["local-skin-preview"] }),
      queryClient.invalidateQueries({ queryKey: ["local-skin-asset"] }),
    ]);
  };

  const chooseDirectory = async () => {
    setActionError(null);
    try {
      const selected = await desktopApi.chooseLocalDirectory(
        source?.data_root ?? source?.install_root,
      );
      if (!selected) return;
      await desktopApi.setLocalSource(client, selected);
      await invalidateLocal();
    } catch (error) {
      setActionError((error as CommandError).message ?? String(error));
    }
  };

  const resetDirectory = async () => {
    setActionError(null);
    try {
      await desktopApi.resetLocalSource(client);
      await invalidateLocal();
    } catch (error) {
      setActionError((error as CommandError).message ?? String(error));
    }
  };

  const scan = async (force: boolean) => {
    setScanning(true);
    setProgress({
      client,
      phase: "discovery",
      processed: 0,
      total: 0,
      percent: 0,
    });
    setActionError(null);
    try {
      await desktopApi.scanLocalSource(client, force);
      await invalidateLocal();
    } catch (error) {
      const commandError = error as CommandError;
      if (commandError.code !== "SCAN_CANCELLED") {
        setActionError(commandError.message ?? String(error));
      }
    } finally {
      setScanning(false);
    }
  };

  const cancel = async () => {
    try {
      await desktopApi.cancelLocalScan(client);
    } catch (error) {
      setActionError((error as CommandError).message ?? String(error));
    }
  };

  return (
    <>
      <PageHeader
        description={
          section === "maps"
            ? "按谱面集浏览本机难度与结构"
            : "浏览 Skin 配置、图像与音效资源"
        }
        eyebrow="Local library"
        title={section === "maps" ? "本地谱面" : "本地皮肤"}
      />

      {indexStatusQuery.data?.phase === "loading" ? (
        <div className="mb-4 flex items-center gap-2 rounded-xl border border-cyan-300/10 bg-cyan-300/[0.05] px-4 py-3 text-sm text-cyan-100">
          <RefreshCw className="size-4 animate-spin" />正在后台加载本地索引，窗口可继续使用…
        </div>
      ) : indexStatusQuery.data?.phase === "error" ? (
        <div className="mb-4 rounded-xl border border-amber-300/10 bg-amber-300/[0.05] px-4 py-3 text-sm text-amber-100">
          本地索引加载失败：{indexStatusQuery.data.error ?? "未知错误"}。可重新扫描以重建索引。
        </div>
      ) : null}

      {sourcesQuery.error ? (
        <ErrorPanel error={sourcesQuery.error} onRetry={() => sourcesQuery.refetch()} />
      ) : (
        <SourceBar
          action={actionError}
          onCancel={cancel}
          onChoose={chooseDirectory}
          onReset={resetDirectory}
          onScan={scan}
          progress={progress}
          scanning={scanning}
          section={section}
          source={source}
          summary={summary}
        />
      )}

      {summaryQuery.isLoading ? (
        <Skeleton className="h-96" />
      ) : summaryQuery.error ? (
        <ErrorPanel error={summaryQuery.error} onRetry={() => summaryQuery.refetch()} />
      ) : !summary ? (
        <EmptyState
          action={
            <Button disabled={!source?.valid} onClick={() => scan(false)} variant="primary">
              <RefreshCw className="size-4" />
              开始扫描
            </Button>
          }
          icon={
            section === "maps" ? (
              <HardDrive className="size-5" />
            ) : (
              <Database className="size-5" />
            )
          }
          title={source?.valid ? "还没有本地索引" : "请先配置数据源"}
          description="完成首次扫描后即可浏览本地资源。"
        />
      ) : section === "maps" ? (
        <BeatmapSetPanel
          client={client}
          onOpen={setSelectedBeatmap}
          ruleset={ruleset}
        />
      ) : (
        <SkinPanel client={client} />
      )}

      {section === "maps" ? (
        <BeatmapDetailDrawer
          client={client}
          onClose={() => setSelectedBeatmap(null)}
          resourceId={selectedBeatmap}
        />
      ) : null}
    </>
  );
}

export function LocalAnalysisPage({
  section = "maps",
}: {
  section?: LocalSection;
}) {
  const { client, ruleset } = useMode();
  return (
    <LocalAnalysisClientPage
      key={`${client}:${ruleset}:${section}`}
      section={section}
    />
  );
}
