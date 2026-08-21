import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { useQueryClient } from "@tanstack/react-query";
import { Download, ExternalLink, FolderOpen, History, LoaderCircle, Map as MapIcon, RefreshCw, Search, Trophy, Upload, X } from "lucide-react";
import { useNavigate, useSearchParams } from "react-router-dom";

import { PageHeader } from "../../shared/components/PageHeader";
import { Button, Card, EmptyState, InfoTip } from "../../shared/components/ui";
import { APP_TIME_ZONE, errorMessage } from "../../shared/lib/format";
import { desktopApi } from "../../shared/lib/tauri";
import type {
  ManiaKeyCount,
  ManiaSimilarityQueryRequest,
  ManiaSimilarityQueryResponse,
  ManiaSimilarityRecommendationResponse,
  ManiaSimilarityResult,
  SimilarityIndexStatus,
  SimilarityRecommendationKind,
} from "../../shared/types/osu";
import { openCollectionDialog } from "../collections/events";
import { normalizePreviewUrl } from "../online-beatmaps/filters";
import { resolveDefaultDownloadProvider } from "../online-beatmaps/downloadProvider";
import { settingsQueryKey, useSettings } from "../settings/api";
import {
  similarityIndexStatusKey,
  similarityRecommendationKey,
  useSimilarityIndexStatus,
  useSimilarityQuery,
  useSimilarityRecommendation,
} from "./api";
import { createManiaSimilarityRequest } from "./defaults";
import {
  onlineBeatmapRouteForSimilarityResult,
  parseSimilarityLaunch,
} from "./navigation";
import {
  excludeTodayRecommendedResults,
  getTodayRecommendationHistory,
  getTodayRecommendedBeatmapIds,
  recordDisplayedRecommendationBatch,
  type RecommendationHistoryEntry,
} from "./recommendationHistory";
import { SimilarityComparisonPanel } from "./SimilarityComparisonPanel";
import { SimilarityRadar } from "./SimilarityRadar";
import { SimilarityResultCard } from "./SimilarityResultCard";
import { formatDataCutoff, similarityIndexStateCopy } from "./viewModel";

const KEY_COUNTS = [4, 6, 7] as const;
const DEFAULT_RESULTS_PER_PAGE = 5;
const ALLOWED_RESULTS_PER_PAGE = [5, 10, 15, 20] as const;

interface ManiaSimilaritySession {
  request: ManiaSimilarityQueryRequest;
  response: ManiaSimilarityQueryResponse | null;
  recommendationResponse: ManiaSimilarityRecommendationResponse | null;
  selectedResultId: number | null;
  activeKeyCount: ManiaKeyCount;
  batches: Record<ManiaKeyCount, number>;
  scrollY: number | null;
}

let maniaSimilaritySession: ManiaSimilaritySession | null = null;

function saveManiaSimilaritySession(session: ManiaSimilaritySession) {
  maniaSimilaritySession = session;
}

function durationLabel(seconds: number) {
  const rounded = Math.max(0, Math.round(seconds));
  return `${Math.floor(rounded / 60)}:${String(rounded % 60).padStart(2, "0")}`;
}

function percentileLabel(value: number) {
  return `${Math.round(value * 100)}%`;
}

function ManiaIndexUnavailable({
  status,
  busy,
  onChoose,
  onRetry,
}: {
  status: SimilarityIndexStatus;
  busy: boolean;
  onChoose: () => void;
  onRetry: () => void;
}) {
  const copy = similarityIndexStateCopy[status.state as Exclude<SimilarityIndexStatus["state"], "ready">];
  return (
    <EmptyState
      action={<div className="flex justify-center gap-2"><Button type="button" variant="primary" onClick={onChoose} disabled={busy}><FolderOpen size={16} />选择 Mania 索引目录</Button><Button type="button" onClick={onRetry} disabled={busy}><RefreshCw size={16} />重新校验</Button></div>}
      description={`${copy.description}${status.message ? ` ${status.message}` : ""}`}
      icon={<a aria-label="查看 Mania 索引说明" href="https://github.com/osuplusplus/osu-difficulty-lab/tree/1fa21fa6a5144992df58efe7ce9d96019981fad3" rel="noreferrer" target="_blank"><ExternalLink size={22} /></a>}
      title={copy.title}
    />
  );
}

export function ManiaSimilarBeatmapsPage() {
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const queryClient = useQueryClient();
  const statusQuery = useSimilarityIndexStatus("mania");
  const similarityQuery = useSimilarityQuery("mania");
  const similarityRecommendation = useSimilarityRecommendation("mania");
  const settings = useSettings();
  const [request, setRequest] = useState<ManiaSimilarityQueryRequest>(() =>
    maniaSimilaritySession?.request ?? createManiaSimilarityRequest({ kind: "beatmap_id", value: "" }),
  );
  const [response, setResponse] = useState<ManiaSimilarityQueryResponse | null>(() => maniaSimilaritySession?.response ?? null);
  const [recommendationResponse, setRecommendationResponse] = useState<ManiaSimilarityRecommendationResponse | null>(() => maniaSimilaritySession?.recommendationResponse ?? null);
  const [selectedResultId, setSelectedResultId] = useState<number | null>(() => maniaSimilaritySession?.selectedResultId ?? null);
  const [activeKeyCount, setActiveKeyCount] = useState<ManiaKeyCount>(() => maniaSimilaritySession?.activeKeyCount ?? 4);
  const [batches, setBatches] = useState<Record<ManiaKeyCount, number>>(() => maniaSimilaritySession?.batches ?? { 4: 0, 6: 0, 7: 0 });
  const [configuring, setConfiguring] = useState(false);
  const [configurationError, setConfigurationError] = useState<string | null>(null);
  const [quickDownloadId, setQuickDownloadId] = useState<number | null>(null);
  const [quickDownloadDirectory, setQuickDownloadDirectory] = useState<string | null>(null);
  const [downloadNotice, setDownloadNotice] = useState<string | null>(null);
  const [playingId, setPlayingId] = useState<number | null>(null);
  const [previewLoadingId, setPreviewLoadingId] = useState<number | null>(null);
  const [recommendationCompleting, setRecommendationCompleting] = useState(false);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [recommendationHistory, setRecommendationHistory] = useState<RecommendationHistoryEntry[]>(() => getTodayRecommendationHistory("mania"));
  const handledLaunch = useRef<string | null>(null);
  const restoreScrollY = useRef(maniaSimilaritySession?.scrollY ?? null);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const recommendationRun = useRef(0);
  const previewVolume = settings.data?.preview_volume ?? 65;
  const configuredResultsPerPage = settings.data?.similarity_preferences?.results_per_page ?? DEFAULT_RESULTS_PER_PAGE;
  const resultsPerPage = ALLOWED_RESULTS_PER_PAGE.includes(configuredResultsPerPage as (typeof ALLOWED_RESULTS_PER_PAGE)[number])
    ? configuredResultsPerPage
    : DEFAULT_RESULTS_PER_PAGE;

  const status = statusQuery.data ?? ({
    ruleset: "mania",
    state: "unconfigured",
    directory: null,
    record_count: null,
    analyzer_version: null,
    normalization_version: null,
    algorithm_id: null,
    data_cutoff_at: null,
    supports_dynamic_weighting: false,
    records_by_key_count: {},
    message: statusQuery.error ? errorMessage(statusQuery.error) : "",
  } satisfies SimilarityIndexStatus);

  const activeGroup = useMemo(
    () => recommendationResponse?.groups.find((group) => group.key_count === activeKeyCount) ?? null,
    [activeKeyCount, recommendationResponse],
  );
  const allResults = useMemo(
    () => recommendationResponse ? activeGroup?.results ?? [] : response?.results ?? [],
    [activeGroup, recommendationResponse, response],
  );
  const resultBatchCount = Math.max(1, Math.ceil(allResults.length / resultsPerPage));
  const activeResultBatch = batches[activeKeyCount] % resultBatchCount;
  const visibleResults = useMemo(
    () => allResults.slice(activeResultBatch * resultsPerPage, (activeResultBatch + 1) * resultsPerPage),
    [activeResultBatch, allResults, resultsPerPage],
  );
  const selected = useMemo(() => {
    if (!visibleResults.length) return null;
    return visibleResults.find((result) => result.beatmap_id === selectedResultId) ?? visibleResults[0];
  }, [selectedResultId, visibleResults]);
  const recommendedBy = selected && recommendationResponse
    ? activeGroup?.results.find((result) => result.beatmap_id === selected.beatmap_id)?.recommended_by ?? null
    : null;
  const comparisonTarget = recommendedBy ?? response?.target ?? null;

  useEffect(() => {
    if (!recommendationResponse || visibleResults.length !== resultsPerPage) return;
    recordDisplayedRecommendationBatch(visibleResults, "mania", resultsPerPage);
  }, [recommendationResponse, resultsPerPage, visibleResults]);

  useEffect(() => {
    saveManiaSimilaritySession({ request, response, recommendationResponse, selectedResultId, activeKeyCount, batches, scrollY: restoreScrollY.current });
  }, [activeKeyCount, batches, recommendationResponse, request, response, selectedResultId]);

  useLayoutEffect(() => {
    const scrollY = restoreScrollY.current;
    if (scrollY == null) return;
    const frame = window.requestAnimationFrame(() => window.scrollTo(0, scrollY));
    return () => window.cancelAnimationFrame(frame);
  }, []);

  useEffect(() => () => { audioRef.current?.pause(); audioRef.current = null; }, []);
  useEffect(() => { if (audioRef.current) audioRef.current.volume = previewVolume / 100; }, [previewVolume]);

  useEffect(() => {
    const launch = parseSimilarityLaunch(searchParams);
    const launchKey = searchParams.toString();
    if (!launch) {
      handledLaunch.current = null;
      return;
    }
    if (launch.ruleset !== "mania" || settings.isLoading || status.state !== "ready" || handledLaunch.current === launchKey) return;
    handledLaunch.current = launchKey;
    const run = async () => {
      const source = launch.kind === "beatmap_id"
        ? { kind: "beatmap_id" as const, value: launch.beatmapId }
        : { kind: "local_file" as const, path: await desktopApi.getLocalBeatmapPath(launch.client, launch.resourceId) };
      const nextRequest = createManiaSimilarityRequest(source);
      setRequest(nextRequest);
      setResponse(null);
      setRecommendationResponse(null);
      setSelectedResultId(null);
      similarityQuery.mutate(nextRequest, { onSuccess: (nextResponse) => {
        if (nextResponse.ruleset !== "mania") return;
        setResponse(nextResponse);
        setActiveKeyCount(nextResponse.target.key_count);
      } });
      setSearchParams(new URLSearchParams(), { replace: true });
    };
    void run().catch((error) => setConfigurationError(errorMessage(error)));
  }, [searchParams, setSearchParams, settings.isLoading, similarityQuery, status.state]);

  async function chooseIndexDirectory() {
    setConfigurationError(null);
    const selectedDirectory = await desktopApi.chooseDirectory("选择 osu!mania 相似谱面索引目录", status.directory ?? undefined);
    if (!selectedDirectory) return;
    setConfiguring(true);
    try {
      const nextStatus = await desktopApi.configureSimilarityIndex("mania", selectedDirectory);
      queryClient.setQueryData(similarityIndexStatusKey("mania"), nextStatus);
      await queryClient.invalidateQueries({ queryKey: settingsQueryKey });
      similarityQuery.reset();
      similarityRecommendation.reset();
      setResponse(null);
      setRecommendationResponse(null);
      setSelectedResultId(null);
    } catch (error) {
      setConfigurationError(errorMessage(error));
    } finally {
      setConfiguring(false);
    }
  }

  function resetResults() {
    recommendationRun.current += 1;
    setRecommendationCompleting(false);
    similarityQuery.reset();
    similarityRecommendation.reset();
    setResponse(null);
    setRecommendationResponse(null);
    setSelectedResultId(null);
    setBatches({ 4: 0, 6: 0, 7: 0 });
  }

  function switchSource(kind: "beatmap_id" | "local_file") {
    resetResults();
    setRequest((current) => ({ ...current, source: kind === "beatmap_id" ? { kind: "beatmap_id", value: "" } : { kind: "local_file", path: "" } }));
  }

  async function chooseOsuFile() {
    const path = await desktopApi.chooseSimilarityBeatmapFile();
    if (path) setRequest((current) => ({ ...current, source: { kind: "local_file", path } }));
  }

  function submit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const value = request.source.kind === "beatmap_id" ? request.source.value.trim() : request.source.path.trim();
    if (!value) return;
    resetResults();
    const nextRequest: ManiaSimilarityQueryRequest = {
      ...request,
      source: request.source.kind === "beatmap_id" ? { kind: "beatmap_id", value } : { kind: "local_file", path: value },
    };
    similarityQuery.mutate(nextRequest, { onSuccess: (nextResponse) => {
      if (nextResponse.ruleset !== "mania") return;
      setResponse(nextResponse);
      setActiveKeyCount(nextResponse.target.key_count);
    } });
  }

  function recommend(kind: SimilarityRecommendationKind) {
    const run = recommendationRun.current + 1;
    recommendationRun.current = run;
    setConfigurationError(null);
    setResponse(null);
    setRecommendationResponse(null);
    setSelectedResultId(null);
    setBatches({ 4: 0, 6: 0, 7: 0 });
    setRecommendationCompleting(false);
    similarityQuery.reset();
    const excludedBeatmapIds = [...getTodayRecommendedBeatmapIds("mania")];
    const quickDisplayedBeatmapIds = new Set<number>();
    const withoutTodayHistory = (nextResponse: ManiaSimilarityRecommendationResponse): ManiaSimilarityRecommendationResponse => ({
      ...nextResponse,
      groups: nextResponse.groups.map((group) => ({
        ...group,
        results: excludeTodayRecommendedResults(
          group.results,
          "mania",
          quickDisplayedBeatmapIds,
        ),
      })),
    });
    const showQuickResponse = (nextResponse: ManiaSimilarityRecommendationResponse) => {
      const visible = withoutTodayHistory(nextResponse);
      const firstVisibleGroup = visible.groups.find((group) => group.results.length);
      for (const group of visible.groups) {
        for (const result of group.results.slice(0, resultsPerPage)) {
          quickDisplayedBeatmapIds.add(result.beatmap_id);
        }
      }
      setRecommendationResponse(visible);
      setActiveKeyCount(firstVisibleGroup?.key_count ?? 4);
    };
    const fullRequest = { ruleset: "mania" as const, kind, result_limit: request.result_limit, excluded_beatmap_ids: excludedBeatmapIds };
    const fullCacheKey = similarityRecommendationKey(fullRequest);
    const complete = (nextResponse: ManiaSimilarityRecommendationResponse) => {
      if (recommendationRun.current !== run) return;
      const visible = withoutTodayHistory(nextResponse);
      queryClient.setQueryData(fullCacheKey, nextResponse);
      setRecommendationResponse(visible);
      setActiveKeyCount(visible.groups.find((group) => group.results.length)?.key_count ?? 4);
      setRecommendationCompleting(false);
    };
    const cached = queryClient.getQueryData<ManiaSimilarityRecommendationResponse>(fullCacheKey);
    if (cached) { complete(cached); return; }

    const quickRequest = { ...fullRequest, result_limit: 5, seed_limit: 5 };
    const quickCacheKey = similarityRecommendationKey(quickRequest);
    const finishInBackground = () => {
      if (fullRequest.result_limit <= 5) return;
      setRecommendationCompleting(true);
      void desktopApi.recommendSimilarBeatmaps(fullRequest).then((nextResponse) => {
        if (nextResponse.ruleset === "mania") complete(nextResponse);
      }).catch(() => { if (recommendationRun.current === run) setRecommendationCompleting(false); });
    };
    const quickCached = queryClient.getQueryData<ManiaSimilarityRecommendationResponse>(quickCacheKey);
    if (quickCached) {
      showQuickResponse(quickCached);
      finishInBackground();
      return;
    }
    similarityRecommendation.mutate(quickRequest, { onSuccess: (nextResponse) => {
      if (recommendationRun.current !== run || nextResponse.ruleset !== "mania") return;
      queryClient.setQueryData(quickCacheKey, nextResponse);
      showQuickResponse(nextResponse);
      finishInBackground();
    } });
  }

  async function downloadResults(results: ManiaSimilarityResult[]) {
    if (!results.length) return;
    let destination = quickDownloadDirectory ?? settings.data?.beatmap_download_directory ?? "";
    if (!destination) {
      destination = await desktopApi.chooseBeatmapDownloadDirectory(null) ?? "";
      if (!destination) return;
      setQuickDownloadDirectory(destination);
      if (settings.data) {
        const saved = await desktopApi.updateSettings({ ...settings.data, beatmap_download_directory: destination });
        queryClient.setQueryData(settingsQueryKey, saved);
      }
    }
    setConfigurationError(null);
    setDownloadNotice(null);
    setQuickDownloadId(results.length === 1 ? results[0].beatmap_id : -1);
    try {
      const downloaded = await desktopApi.downloadOnlineBeatmapsets({
        destination,
        provider: resolveDefaultDownloadProvider(settings.data),
        overwrite: false,
        include_video: settings.data?.include_video_in_beatmap_downloads ?? true,
        items: Array.from(new Map(results.map((result) => [result.beatmapset_id, { beatmapset_id: result.beatmapset_id, artist: result.artist, title: result.title }])).values()),
      });
      setDownloadNotice(downloaded.completed > 0 ? `已下载 ${downloaded.completed} 个谱面集到：${downloaded.destination}` : `下载已处理；保存位置：${downloaded.destination}`);
    } catch (error) {
      setConfigurationError(errorMessage(error));
    } finally {
      setQuickDownloadId(null);
    }
  }

  function openOnlineBeatmap(result: ManiaSimilarityResult) {
    saveManiaSimilaritySession({ request, response, recommendationResponse, selectedResultId, activeKeyCount, batches, scrollY: window.scrollY || null });
    navigate(onlineBeatmapRouteForSimilarityResult(result), { state: { returnTo: "/online/similar" } });
  }

  async function togglePreview(result: ManiaSimilarityResult) {
    if (playingId === result.beatmap_id && audioRef.current) {
      audioRef.current.pause(); audioRef.current = null; setPlayingId(null); return;
    }
    setPreviewLoadingId(result.beatmap_id);
    try {
      const beatmapset = await desktopApi.getOnlineBeatmapset(result.beatmapset_id);
      const source = normalizePreviewUrl(beatmapset.preview_url);
      if (!source) return;
      audioRef.current?.pause();
      const audio = new Audio(source);
      audio.volume = previewVolume / 100;
      audio.onended = () => setPlayingId(null);
      audio.onerror = () => setPlayingId(null);
      audioRef.current = audio;
      setPlayingId(result.beatmap_id);
      await audio.play();
    } catch (error) {
      setConfigurationError(errorMessage(error));
      audioRef.current = null;
      setPlayingId(null);
    } finally {
      setPreviewLoadingId(null);
    }
  }

  function showNextBatch() {
    const nextBatch = (activeResultBatch + 1) % resultBatchCount;
    setBatches((current) => ({ ...current, [activeKeyCount]: nextBatch }));
    setSelectedResultId(allResults[nextBatch * resultsPerPage]?.beatmap_id ?? null);
  }

  if (statusQuery.isLoading) {
    return <><PageHeader title="相似谱面" description="从本地私有索引中寻找特征相近的 osu!mania 谱面。" /><EmptyState description="正在以只读方式检查本机配置。" icon={<RefreshCw className="animate-spin" size={22} />} title="正在校验 Mania 索引" /></>;
  }

  return (
    <>
      <Dialog.Root open={historyOpen} onOpenChange={(open) => { setHistoryOpen(open); if (open) setRecommendationHistory(getTodayRecommendationHistory("mania")); }}>
        <Dialog.Portal>
          <Dialog.Overlay className="fixed inset-0 z-[260] bg-black/70 backdrop-blur-sm" />
          <Dialog.Content className="fixed left-1/2 top-1/2 z-[270] flex max-h-[min(760px,calc(100vh-32px))] w-[min(760px,calc(100vw-32px))] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-2xl border border-cyan-300/20 bg-[#101724] shadow-2xl outline-none">
            <div className="flex items-start justify-between gap-4 border-b border-white/[0.08] p-6"><div><Dialog.Title className="text-lg font-semibold text-white">今日 Mania 推荐历史</Dialog.Title><Dialog.Description className="mt-1 text-sm text-slate-400">按 Mania 模式独立记录，共 {recommendationHistory.length} 张。</Dialog.Description></div><Dialog.Close aria-label="关闭今日推荐历史" className="text-slate-500 transition hover:text-white"><X className="size-5" /></Dialog.Close></div>
            <div className="min-h-0 flex-1 overflow-y-auto p-5">
              {recommendationHistory.length ? <div className="space-y-2">{recommendationHistory.map(({ displayed_at, key_count, result }) => <button className="flex w-full items-center gap-4 rounded-xl border border-white/[0.06] bg-black/10 px-4 py-3 text-left transition hover:border-cyan-300/20 hover:bg-white/[0.04]" key={result.beatmap_id} onClick={() => { setHistoryOpen(false); if (result.ruleset === "mania") openOnlineBeatmap(result); }} type="button"><div className="min-w-0 flex-1"><p className="truncate text-sm font-medium text-slate-100">{result.artist} - {result.title}</p><p className="mt-1 truncate text-xs text-slate-500">{key_count}K · [{result.version}] · {result.creator}</p></div><time className="shrink-0 text-xs text-slate-500">{new Intl.DateTimeFormat("zh-CN", { hour: "2-digit", minute: "2-digit", timeZone: APP_TIME_ZONE }).format(new Date(displayed_at))}</time></button>)}</div> : <EmptyState title="今天还没有 Mania 推荐记录" description="当一页推荐谱面完整展示后，会自动出现在这里。" />}
            </div>
          </Dialog.Content>
        </Dialog.Portal>
      </Dialog.Root>

      <PageHeader
        title="相似谱面"
        description="使用独立的 osu!mania Analyzer v1 索引，按同键数比较强度、键型、结构、难度分位及基础上下文。"
        actions={<div className="flex items-center gap-2"><Button onClick={() => navigate("/local/maps")} size="sm" variant="secondary"><MapIcon className="size-3.5" />前往本地谱面</Button><InfoTip text="首版仅支持 NoMod 4K、6K 与 7K；难度分位表示同键数 Ranked 谱面中的相对位置，并非官方星数。" /></div>}
      />

      {configurationError ? <div className="mb-5 rounded-xl border border-rose-300/20 bg-rose-300/10 px-4 py-3 text-sm text-rose-100">{configurationError}</div> : null}
      {downloadNotice ? <div className="mb-5 rounded-xl border border-emerald-300/20 bg-emerald-300/10 px-4 py-3 text-sm text-emerald-100">{downloadNotice}</div> : null}

      {status.state !== "ready" ? (
        <ManiaIndexUnavailable status={status} busy={configuring || statusQuery.isFetching} onChoose={() => void chooseIndexDirectory()} onRetry={() => void statusQuery.refetch()} />
      ) : (
        <>
          <Card className="mb-4 flex items-center justify-between gap-4 border-white/[0.055] bg-black/[0.06] px-4 py-2.5">
            <div className="min-w-0 text-xs text-slate-500"><span className="mr-2 inline-flex items-center gap-1.5 text-slate-400"><span className="size-1.5 rounded-full bg-emerald-400/70" />Mania 索引已就绪</span><span>{status.record_count == null ? "已通过本机只读校验" : `共 ${status.record_count.toLocaleString()} 条记录`}{KEY_COUNTS.map((keyCount) => status.records_by_key_count?.[keyCount] == null ? "" : ` · ${keyCount}K ${status.records_by_key_count[keyCount]!.toLocaleString()}`).join("")}{status.analyzer_version == null ? "" : ` · Analyzer v${status.analyzer_version}`} · {formatDataCutoff(status.data_cutoff_at)}</span></div>
            <div className="flex gap-2"><Button type="button" size="sm" variant="ghost" onClick={() => void statusQuery.refetch()} disabled={statusQuery.isFetching}><RefreshCw size={14} />重新校验</Button><Button type="button" size="sm" variant="ghost" onClick={() => void chooseIndexDirectory()} disabled={configuring}><FolderOpen size={14} />更换目录</Button></div>
          </Card>

          <Card className="mb-5 p-5">
            <div className="mb-5 border-b border-white/[0.07] pb-5">
              <div className="mb-3"><span className="text-[10px] font-bold uppercase tracking-[0.18em] text-[var(--theme-primary)]">为你推荐</span><p className="mt-1 text-xs text-slate-400">最近成绩与 BP 会按 4K、6K、7K 分组，每组独立推荐和分页。</p></div>
              <div className="flex flex-wrap gap-2"><Button type="button" variant="primary" disabled={similarityQuery.isPending || similarityRecommendation.isPending} loading={similarityRecommendation.isPending && similarityRecommendation.variables?.kind === "recent"} onClick={() => recommend("recent")}><History size={16} />根据最近游玩推荐</Button><Button type="button" disabled={similarityQuery.isPending || similarityRecommendation.isPending} loading={similarityRecommendation.isPending && similarityRecommendation.variables?.kind === "best"} onClick={() => recommend("best")}><Trophy size={16} />根据你的 BP 推荐</Button><Button type="button" variant="ghost" onClick={() => { setRecommendationHistory(getTodayRecommendationHistory("mania")); setHistoryOpen(true); }}><History size={16} />今日推荐历史</Button></div>
            </div>
            <form onSubmit={submit}>
              <div className="mb-5 inline-flex rounded-lg border border-white/[0.08] bg-black/15 p-1" role="tablist" aria-label="参考谱面输入方式"><Button type="button" role="tab" aria-selected={request.source.kind === "beatmap_id"} size="sm" variant={request.source.kind === "beatmap_id" ? "primary" : "ghost"} onClick={() => switchSource("beatmap_id")}>ID / 链接</Button><Button type="button" role="tab" aria-selected={request.source.kind === "local_file"} size="sm" variant={request.source.kind === "local_file" ? "primary" : "ghost"} onClick={() => switchSource("local_file")}>本地 .osu</Button></div>
              <div className="flex items-end gap-3">
                <label className="min-w-0 flex-1 text-xs text-slate-400"><span className="mb-1.5 block">{request.source.kind === "beatmap_id" ? "Beatmap ID 或 osu! 链接" : "osu!mania 谱面文件"}</span><input className="opp-input" value={request.source.kind === "beatmap_id" ? request.source.value : request.source.path} placeholder={request.source.kind === "beatmap_id" ? "例如 1234567 或 https://osu.ppy.sh/beatmaps/1234567" : "选择一个 4K、6K 或 7K .osu 文件"} readOnly={request.source.kind === "local_file"} onChange={(event) => setRequest((current) => ({ ...current, source: { kind: "beatmap_id", value: event.target.value } }))} /></label>
                {request.source.kind === "local_file" ? <Button type="button" onClick={() => void chooseOsuFile()}><Upload size={16} />选择文件</Button> : null}
                <Button variant="primary" type="submit" disabled={!(request.source.kind === "beatmap_id" ? request.source.value : request.source.path).trim() || similarityQuery.isPending || similarityRecommendation.isPending}><Search size={16} />{similarityQuery.isPending ? "检索中…" : "查找相似谱面"}</Button>
              </div>
            </form>
          </Card>

          {similarityQuery.error || similarityRecommendation.error ? <div className="mb-5 rounded-xl border border-rose-300/20 bg-rose-300/10 px-4 py-3 text-sm text-rose-100">{errorMessage(similarityQuery.error ?? similarityRecommendation.error)}</div> : null}

          {response || recommendationResponse ? (
            <section className="grid items-start gap-5 xl:grid-cols-[minmax(0,1fr)_380px]">
              <div className="min-w-0">
                {recommendationResponse ? (
                  <Card className="mb-5 p-5"><span className="text-[10px] font-bold uppercase tracking-[0.18em] text-[var(--theme-primary)]">Mania 个性化推荐</span><h2 className="mt-2 text-lg font-semibold text-white">{recommendationResponse.kind === "recent" ? "根据最近游玩生成" : "根据你的 BP 生成"}</h2><p className="mt-1 text-sm text-slate-400">已使用 {recommendationResponse.seed_count} 张参考谱面{recommendationResponse.skipped_seed_count ? `，跳过 ${recommendationResponse.skipped_seed_count} 张不支持或无法读取的谱面` : ""}</p>{recommendationCompleting ? <p className="mt-3 flex items-center gap-2 text-xs text-[var(--theme-primary-light)]"><LoaderCircle className="size-3.5 animate-spin" />已按键数优先展示首批结果，正在后台完善更多推荐</p> : null}</Card>
                ) : response ? (
                  <Card className="similarity-reference-summary mb-4 grid items-center gap-3 px-5 py-4 sm:grid-cols-[minmax(0,1fr)_230px]"><div><span className="text-[10px] font-bold uppercase tracking-[0.18em] text-[var(--theme-primary)]">Mania 参考谱面</span><h2 className="mt-2 text-lg font-semibold text-white">{response.target.key_count}K · {response.target.version || "本地谱面"}</h2><p className="mt-1 text-sm text-slate-400">{response.target.artist} — {response.target.title}</p><div className="mt-3 flex flex-wrap gap-x-3 gap-y-1 text-xs text-slate-400"><span>{response.target.family.toUpperCase()} / {response.target.pattern}</span><span>同键数难度分位 {percentileLabel(response.target.difficulty_percentile)}</span><span>BPM {Math.round(response.target.base.bpm)}</span><span>有效长度 {durationLabel(response.target.base.active_length_seconds)}</span></div></div><SimilarityRadar compact target={response.target.difficulty} /></Card>
                ) : null}

                {recommendationResponse ? <div className="mb-4 inline-flex rounded-lg border border-white/[0.08] bg-black/15 p-1" role="tablist" aria-label="Mania 键数分组">{KEY_COUNTS.map((keyCount) => { const group = recommendationResponse.groups.find((item) => item.key_count === keyCount); return <Button aria-selected={activeKeyCount === keyCount} key={keyCount} onClick={() => { setActiveKeyCount(keyCount); setSelectedResultId(group?.results[0]?.beatmap_id ?? null); }} role="tab" size="sm" type="button" variant={activeKeyCount === keyCount ? "primary" : "ghost"}>{keyCount}K · {group?.results.length ?? 0}<span className="ml-1 text-[10px] opacity-60">({group?.seed_count ?? 0} seeds)</span></Button>; })}</div> : null}

                <div className="mb-3 flex items-end justify-between gap-3"><div><span className="text-[10px] font-bold uppercase tracking-[0.18em] text-slate-500">推荐结果</span><h2 className="mt-1 text-base font-semibold text-white">{allResults.length} 个 {activeKeyCount}K 相似谱面集</h2></div><div className="flex flex-wrap items-center justify-end gap-2"><span className="text-sm text-slate-500">第 {activeResultBatch + 1} / {resultBatchCount} 批</span>{allResults.length > resultsPerPage ? <Button disabled={quickDownloadId !== null} onClick={showNextBatch} size="sm"><RefreshCw className="size-3.5" />换一批</Button> : null}<Button disabled={!visibleResults.length || quickDownloadId !== null} loading={quickDownloadId === -1} onClick={() => void downloadResults(visibleResults)} size="sm" variant="primary"><Download className="size-3.5" />下载本批</Button></div></div>

                {allResults.length ? <div className="space-y-3">{visibleResults.map((result) => <SimilarityResultCard key={result.beatmap_id} result={result} recommendedBy={activeGroup?.results.find((item) => item.beatmap_id === result.beatmap_id)?.recommended_by} selected={selected?.beatmap_id === result.beatmap_id} onSelect={() => setSelectedResultId(result.beatmap_id)} onDownload={() => void downloadResults([result])} onAddToCollection={() => openCollectionDialog([{ beatmap_id: result.beatmap_id, beatmapset_id: result.beatmapset_id, checksum: null, ruleset: result.ruleset, difficulty_name: result.version, title: result.title, artist: result.artist, creator: result.creator }])} downloading={quickDownloadId === result.beatmap_id} downloadDisabled={quickDownloadId !== null} onOpen={() => openOnlineBeatmap(result)} onPreview={() => void togglePreview(result)} playing={playingId === result.beatmap_id} previewLoading={previewLoadingId === result.beatmap_id} />)}</div> : <EmptyState title={`没有可展示的 ${activeKeyCount}K 推荐`} description="该键数组可能没有可用参考成绩，或今天已展示过全部候选；可切换其他键数组。" />}
              </div>

              {selected && comparisonTarget ? <SimilarityComparisonPanel selected={selected} target={comparisonTarget} recommendedBy={recommendedBy} dynamicProfile={null} onOpen={() => openOnlineBeatmap(selected)} /> : null}
            </section>
          ) : null}
        </>
      )}
    </>
  );
}
