import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { useQueryClient } from "@tanstack/react-query";
import { Download, ExternalLink, FolderOpen, History, LoaderCircle, Map as MapIcon, RefreshCw, Search, Trophy, Upload, X } from "lucide-react";
import { useNavigate, useSearchParams } from "react-router-dom";

import { useMode } from "../../app/ModeContext";
import { PageHeader } from "../../shared/components/PageHeader";
import { Button, Card, EmptyState, InfoTip } from "../../shared/components/ui";
import { APP_TIME_ZONE, errorMessage } from "../../shared/lib/format";
import { settingsQueryKey, useSettings } from "../settings/api";
import { desktopApi } from "../../shared/lib/tauri";
import type {
  SimilarityIndexStatus,
  OsuSimilarityQueryRequest,
  OsuSimilarityQueryResponse,
  OsuSimilarityRecommendationResponse,
  Ruleset,
  SimilarityRecommendationKind,
  SimilarityRecommendationResult,
  SimilarityResult,
} from "../../shared/types/osu";
import {
  similarityIndexStatusKey,
  similarityRecommendationKey,
  useSimilarityIndexStatus,
  useSimilarityQuery,
  useSimilarityRecommendation,
} from "./api";
import {
  createSimilarityRequest,
  defaultDynamicWeighting,
  defaultSimilarityPreferences,
  manualWeightingFromPreferences,
} from "./defaults";
import { SimilarityAdvancedPanel } from "./SimilarityAdvancedPanel";
import { SimilarityFilterSliders } from "./SimilarityFilterSliders";
import { SimilarityRadar } from "./SimilarityRadar";
import { SimilarityResultCard } from "./SimilarityResultCard";
import { SimilarityComparisonPanel } from "./SimilarityComparisonPanel";
import { ManiaSimilarBeatmapsPage } from "./ManiaSimilarBeatmapsPage";
import {
  onlineBeatmapRouteForSimilarityResult,
  parseSimilarityLaunch,
} from "./navigation";
import { normalizePreviewUrl } from "../online-beatmaps/filters";
import { resolveDefaultDownloadProvider } from "../online-beatmaps/downloadProvider";
import { openCollectionDialog } from "../collections/events";
import {
  formatDataCutoff,
  formatSimilarityMetric,
  matchesCandidateFilters,
  resolveSimilarityWeighting,
  similarityIndexStateCopy,
} from "./viewModel";
import {
  excludeTodayRecommendedResults,
  getTodayRecommendationHistory,
  getTodayRecommendedBeatmapIds,
  recordDisplayedRecommendationBatch,
  type RecommendationHistoryEntry,
} from "./recommendationHistory";

const DEFAULT_RESULTS_PER_PAGE = 5;
const ALLOWED_RESULTS_PER_PAGE = [5, 10, 15, 20] as const;

function manualWeighting() {
  return manualWeightingFromPreferences();
}

interface SimilaritySession {
  request: OsuSimilarityQueryRequest;
  response: OsuSimilarityQueryResponse | null;
  recommendationResponse: OsuSimilarityRecommendationResponse | null;
  selectedResultId: number | null;
  advancedOpen: boolean;
  scrollY: number | null;
}

let standardSimilaritySession: SimilaritySession | null = null;

function saveSimilaritySession(session: SimilaritySession) {
  standardSimilaritySession = session;
}

function IndexUnavailable({
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
      action={
        <div className="flex justify-center gap-2">
          <Button type="button" variant="primary" onClick={onChoose} disabled={busy}>
            <FolderOpen size={16} aria-hidden="true" />
            选择索引目录
          </Button>
          <Button type="button" onClick={onRetry} disabled={busy}>
            <RefreshCw size={16} aria-hidden="true" />
            重新校验
          </Button>
        </div>
      }
      description={`${copy.description}${status.message ? ` ${status.message}` : ""}`}
      icon={<a aria-label="下载相似谱面索引" href="https://github.com/osuplusplus/osu-difficulty-lab/releases" rel="noreferrer" target="_blank" title="下载相似谱面索引"><ExternalLink size={22} aria-hidden="true" /></a>}
      title={copy.title}
    />
  );
}

function StandardSimilarBeatmapsPage() {
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const queryClient = useQueryClient();
  const statusQuery = useSimilarityIndexStatus("osu");
  const similarityQuery = useSimilarityQuery("osu");
  const similarityRecommendation = useSimilarityRecommendation("osu");
  const settings = useSettings();
  const [request, setRequest] = useState<OsuSimilarityQueryRequest>(() =>
    standardSimilaritySession?.request ?? createSimilarityRequest({ kind: "beatmap_id", value: "" }),
  );
  const [advancedOpen, setAdvancedOpen] = useState(() => standardSimilaritySession?.advancedOpen ?? false);
  const [configuring, setConfiguring] = useState(false);
  const [configurationError, setConfigurationError] = useState<string | null>(null);
  const [quickDownloadId, setQuickDownloadId] = useState<number | null>(null);
  const [quickDownloadDirectory, setQuickDownloadDirectory] = useState<string | null>(null);
  const [downloadNotice, setDownloadNotice] = useState<string | null>(null);
  const [playingId, setPlayingId] = useState<number | null>(null);
  const [previewLoadingId, setPreviewLoadingId] = useState<number | null>(null);
  const [response, setResponse] = useState<OsuSimilarityQueryResponse | null>(
    () => standardSimilaritySession?.response ?? null,
  );
  const [recommendationResponse, setRecommendationResponse] =
    useState<OsuSimilarityRecommendationResponse | null>(
      () => standardSimilaritySession?.recommendationResponse ?? null,
    );
  const [recommendationCompleting, setRecommendationCompleting] = useState(false);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [recommendationHistory, setRecommendationHistory] = useState<RecommendationHistoryEntry[]>(
    () => getTodayRecommendationHistory("osu"),
  );
  const [selectedResultId, setSelectedResultId] = useState<number | null>(
    () => standardSimilaritySession?.selectedResultId ?? null,
  );
  const [resultBatch, setResultBatch] = useState(0);
  const handledLaunch = useRef<string | null>(null);
  const restoreScrollY = useRef(standardSimilaritySession?.scrollY ?? null);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const preferenceSaveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const recommendationRun = useRef(0);
  const previewVolume = settings.data?.preview_volume ?? 65;
  const preferences = useMemo(() => ({
    ...defaultSimilarityPreferences,
    ...settings.data?.similarity_preferences,
    manual_weights: {
      ...defaultSimilarityPreferences.manual_weights,
      ...settings.data?.similarity_preferences?.manual_weights,
    },
  }), [settings.data?.similarity_preferences]);
  const resultsPerPage = ALLOWED_RESULTS_PER_PAGE.includes(
    preferences.results_per_page as (typeof ALLOWED_RESULTS_PER_PAGE)[number],
  ) ? preferences.results_per_page : DEFAULT_RESULTS_PER_PAGE;

  const filteredResults = useMemo(
    () => (recommendationResponse?.results ?? response?.results ?? []).filter(
      (result) => matchesCandidateFilters(result.base, result.star_rating, request.filters),
    ),
    [recommendationResponse, request.filters, response],
  );
  const resultBatchCount = Math.max(1, Math.ceil(filteredResults.length / resultsPerPage));
  const activeResultBatch = resultBatch % resultBatchCount;
  const visibleResults = useMemo(
    () => filteredResults.slice(
      activeResultBatch * resultsPerPage,
      (activeResultBatch + 1) * resultsPerPage,
    ),
    [activeResultBatch, filteredResults, resultsPerPage],
  );

  useEffect(() => {
    if (!recommendationResponse || visibleResults.length !== resultsPerPage) return;
    recordDisplayedRecommendationBatch(visibleResults as SimilarityRecommendationResult[], "osu", resultsPerPage);
  }, [recommendationResponse, resultsPerPage, visibleResults]);

  const selected = useMemo(() => {
    if (!visibleResults.length) return null;
    if (!selectedResultId) return visibleResults[0];
    return (
      visibleResults.find(
        (result) => result.beatmap_id === selectedResultId,
      ) ?? visibleResults[0]
    );
  }, [selectedResultId, visibleResults]);
  const recommendedBy = selected
    ? recommendationResponse?.results.find(
        (result) => result.beatmap_id === selected.beatmap_id,
      )?.recommended_by ?? null
    : null;
  const comparisonTarget = recommendedBy ?? response?.target ?? null;
  const selectedDynamicProfile = selected
    ? response?.dynamic_profile ?? recommendationResponse?.dynamic_profiles.find(
        (profile) => profile.seed_beatmap_id === recommendedBy?.beatmap_id,
      ) ?? null
    : null;

  const status =
    statusQuery.data ??
    ({
      ruleset: "osu",
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
  const effectiveWeighting = resolveSimilarityWeighting(
    request,
    preferences,
    status.supports_dynamic_weighting,
  );

  useEffect(() => () => {
    if (preferenceSaveTimer.current) clearTimeout(preferenceSaveTimer.current);
  }, []);

  function changeAdvancedRequest(next: OsuSimilarityQueryRequest) {
    setRequest(next);
    if (!settings.data || !preferences.advanced_enabled) return;
    const nextPreferences = next.weighting.mode === "dynamic"
      ? { ...preferences, mode: "dynamic" as const, lower_sections: next.weighting.lower_sections, upper_sections: next.weighting.upper_sections }
      : { ...preferences, mode: "manual" as const, manual_weights: { ...next.weighting.difficulty_weights, parameters: next.weighting.parameter_weight } };
    const nextSettings = { ...settings.data, similarity_preferences: nextPreferences };
    // Treat the query cache as the local preference draft so a mode switch during
    // the debounce window restores the newest values instead of stale persisted ones.
    queryClient.setQueryData(settingsQueryKey, nextSettings);
    if (preferenceSaveTimer.current) clearTimeout(preferenceSaveTimer.current);
    preferenceSaveTimer.current = setTimeout(() => {
      void desktopApi.updateSettings(nextSettings).then((saved) => {
        queryClient.setQueryData(settingsQueryKey, saved);
      });
    }, 400);
  }

  useEffect(() => {
    saveSimilaritySession({
      request,
      response,
      recommendationResponse,
      selectedResultId,
      advancedOpen,
      scrollY: restoreScrollY.current,
    });
  }, [advancedOpen, recommendationResponse, request, response, selectedResultId]);

  useLayoutEffect(() => {
    const scrollY = restoreScrollY.current;
    if (scrollY == null) return;
    const frame = window.requestAnimationFrame(() => window.scrollTo(0, scrollY));
    return () => window.cancelAnimationFrame(frame);
  }, []);

  useEffect(() => () => {
    audioRef.current?.pause();
    audioRef.current = null;
  }, []);

  useEffect(() => {
    if (audioRef.current) audioRef.current.volume = previewVolume / 100;
  }, [previewVolume]);

  useEffect(() => {
    const launch = parseSimilarityLaunch(searchParams);
    const launchKey = searchParams.toString();
    if (!launch) {
      handledLaunch.current = null;
      return;
    }
    if (
      launch.ruleset !== "osu" ||
      settings.isLoading ||
      status.state !== "ready" ||
      handledLaunch.current === launchKey
    ) return;
    handledLaunch.current = launchKey;

    const run = async () => {
      const source =
        launch.kind === "beatmap_id"
          ? { kind: "beatmap_id" as const, value: launch.beatmapId }
          : {
              kind: "local_file" as const,
              path: await desktopApi.getLocalBeatmapPath(launch.client, launch.resourceId),
            };
      const launchWeighting = preferences.advanced_enabled
        ? preferences.mode === "dynamic" && status.supports_dynamic_weighting
          ? {
              mode: "dynamic" as const,
              lower_sections: preferences.lower_sections,
              upper_sections: preferences.upper_sections,
            }
          : manualWeightingFromPreferences(preferences)
        : status.supports_dynamic_weighting
          ? { ...defaultDynamicWeighting }
          : manualWeighting();
      const nextRequest = {
        ...createSimilarityRequest(source),
        weighting: launchWeighting,
      };
      setRequest(nextRequest);
      setResponse(null);
      setRecommendationResponse(null);
      setSelectedResultId(null);
      similarityQuery.mutate(nextRequest, {
        onSuccess: (nextResponse) => {
          if (nextResponse.ruleset === "osu") setResponse(nextResponse);
        },
      });
      setSearchParams(new URLSearchParams(), { replace: true });
    };
    void run().catch((error) => setConfigurationError(errorMessage(error)));
  }, [preferences, searchParams, setSearchParams, settings.isLoading, similarityQuery, status.state, status.supports_dynamic_weighting]);

  async function chooseIndexDirectory() {
    setConfigurationError(null);
    const selectedDirectory = await desktopApi.chooseDirectory(
      "选择相似谱面索引目录",
      statusQuery.data?.directory ?? undefined,
    );
    if (!selectedDirectory) return;

    setConfiguring(true);
    try {
      const status = await desktopApi.configureSimilarityIndex("osu", selectedDirectory);
      queryClient.setQueryData(similarityIndexStatusKey("osu"), status);
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

  function switchSource(kind: "beatmap_id" | "local_file") {
    recommendationRun.current += 1;
    setRecommendationCompleting(false);
    similarityQuery.reset();
    similarityRecommendation.reset();
    setResponse(null);
    setRecommendationResponse(null);
    setSelectedResultId(null);
    setRequest((current) => ({
      ...current,
      source:
        kind === "beatmap_id"
          ? { kind: "beatmap_id", value: "" }
          : { kind: "local_file", path: "" },
    }));
  }

  async function chooseOsuFile() {
    const path = await desktopApi.chooseSimilarityBeatmapFile();
    if (!path) return;
    setRequest((current) => ({
      ...current,
      source: { kind: "local_file", path },
    }));
  }

  async function downloadResults(results: SimilarityResult[]) {
    if (!results.length) return;
    let destination =
      quickDownloadDirectory ?? settings.data?.beatmap_download_directory ?? "";
    if (!destination) {
      destination = await desktopApi.chooseBeatmapDownloadDirectory(null) ?? "";
      if (!destination) return;
      setQuickDownloadDirectory(destination);
      if (settings.data) {
        const saved = await desktopApi.updateSettings({
          ...settings.data,
          beatmap_download_directory: destination,
        });
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
      setDownloadNotice(
        downloaded.completed > 0
          ? `已下载 ${downloaded.completed} 个谱面集到：${downloaded.destination}`
          : `下载已处理；保存位置：${downloaded.destination}`,
      );
    } catch (error) {
      setConfigurationError(errorMessage(error));
    } finally {
      setQuickDownloadId(null);
    }
  }

  async function downloadResult(result: SimilarityResult) {
    await downloadResults([result]);
  }

  function openOnlineBeatmap(result: SimilarityResult) {
    saveSimilaritySession({
      request,
      response,
      recommendationResponse,
      selectedResultId,
      advancedOpen,
      scrollY: window.scrollY || null,
    });
    navigate(onlineBeatmapRouteForSimilarityResult(result), {
      state: { returnTo: "/online/similar" },
    });
  }

  async function togglePreview(result: SimilarityResult) {
    if (playingId === result.beatmap_id && audioRef.current) {
      audioRef.current.pause();
      audioRef.current = null;
      setPlayingId(null);
      return;
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

  function submit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const value =
      request.source.kind === "beatmap_id"
        ? request.source.value.trim()
        : request.source.path.trim();
    if (!value) return;
    setResultBatch(0);
    setSelectedResultId(null);
    setResponse(null);
    setRecommendationResponse(null);
    recommendationRun.current += 1;
    setRecommendationCompleting(false);
    similarityRecommendation.reset();
    similarityQuery.mutate({
      ...request,
      weighting: effectiveWeighting,
      filters: { ...request.filters },
      source:
        request.source.kind === "beatmap_id"
          ? { kind: "beatmap_id", value }
          : { kind: "local_file", path: value },
    }, {
      onSuccess: (nextResponse) => {
        if (nextResponse.ruleset === "osu") setResponse(nextResponse);
      },
    });
  }

  function recommend(kind: SimilarityRecommendationKind) {
    const run = recommendationRun.current + 1;
    recommendationRun.current = run;
    setConfigurationError(null);
    setResultBatch(0);
    setSelectedResultId(null);
    setResponse(null);
    setRecommendationResponse(null);
    setRecommendationCompleting(false);
    similarityQuery.reset();
    const excludedBeatmapIds = [...getTodayRecommendedBeatmapIds("osu")];
    const quickDisplayedBeatmapIds = new Set<number>();
    const withoutTodayHistory = (nextResponse: OsuSimilarityRecommendationResponse) => ({
      ...nextResponse,
      results: excludeTodayRecommendedResults(
        nextResponse.results,
        "osu",
        quickDisplayedBeatmapIds,
      ),
    });
    const showQuickResponse = (nextResponse: OsuSimilarityRecommendationResponse) => {
      const visible = withoutTodayHistory(nextResponse);
      const displayed = visible.results
        .filter((result) => matchesCandidateFilters(result.base, result.star_rating, request.filters))
        .slice(0, resultsPerPage);
      for (const result of displayed) quickDisplayedBeatmapIds.add(result.beatmap_id);
      setRecommendationResponse(visible);
    };
    const fullRequest = {
      ruleset: "osu" as const,
      kind,
      weighting: effectiveWeighting,
      filters: { ...request.filters },
      result_limit: request.result_limit,
      excluded_beatmap_ids: excludedBeatmapIds,
    };
    const fullCacheKey = similarityRecommendationKey(fullRequest);
    const complete = (nextResponse: OsuSimilarityRecommendationResponse) => {
      if (recommendationRun.current !== run) return;
      queryClient.setQueryData(fullCacheKey, nextResponse);
      setRecommendationResponse(withoutTodayHistory(nextResponse));
      setRecommendationCompleting(false);
    };
    const cached = queryClient.getQueryData<OsuSimilarityRecommendationResponse>(fullCacheKey);
    if (cached) {
      complete(cached);
      return;
    }

    const quickRequest = { ...fullRequest, result_limit: 5, seed_limit: 5 };
    const quickCacheKey = similarityRecommendationKey(quickRequest);
    const finishInBackground = () => {
      if (fullRequest.result_limit <= 5) return;
      setRecommendationCompleting(true);
      void desktopApi.recommendSimilarBeatmaps(fullRequest)
        .then((nextResponse) => {
          if (nextResponse.ruleset === "osu") complete(nextResponse);
        })
        .catch(() => {
          if (recommendationRun.current === run) setRecommendationCompleting(false);
        });
    };
    const quickCached = queryClient.getQueryData<OsuSimilarityRecommendationResponse>(quickCacheKey);
    if (quickCached) {
      if (recommendationRun.current === run) showQuickResponse(quickCached);
      finishInBackground();
      return;
    }
    similarityRecommendation.mutate(quickRequest, {
      onSuccess: (nextResponse) => {
        if (recommendationRun.current !== run || nextResponse.ruleset !== "osu") return;
        queryClient.setQueryData(quickCacheKey, nextResponse);
        showQuickResponse(nextResponse);
        finishInBackground();
      },
    });
  }

  function showNextBatch() {
    const nextBatch = (activeResultBatch + 1) % resultBatchCount;
    setResultBatch(nextBatch);
    setSelectedResultId(filteredResults[nextBatch * resultsPerPage]?.beatmap_id ?? null);
  }

  if (statusQuery.isLoading) {
    return (
      <>
        <PageHeader title="相似谱面" description="从本地私有索引中寻找特征相近的 osu!standard 谱面。" />
        <EmptyState
          description="正在以只读方式检查本机配置。"
          icon={<RefreshCw className="animate-spin" size={22} aria-hidden="true" />}
          title="正在校验本地索引"
        />
      </>
    );
  }

  return (
    <>
      <Dialog.Root open={historyOpen} onOpenChange={(open) => {
        setHistoryOpen(open);
        if (open) setRecommendationHistory(getTodayRecommendationHistory("osu"));
      }}>
        <Dialog.Portal>
          <Dialog.Overlay className="fixed inset-0 z-[260] bg-black/70 backdrop-blur-sm" />
          <Dialog.Content className="fixed left-1/2 top-1/2 z-[270] flex max-h-[min(760px,calc(100vh-32px))] w-[min(760px,calc(100vw-32px))] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-2xl border border-cyan-300/20 bg-[#101724] shadow-2xl outline-none">
            <div className="flex items-start justify-between gap-4 border-b border-white/[0.08] p-6">
              <div>
                <Dialog.Title className="text-lg font-semibold text-white">今日推荐历史</Dialog.Title>
                <Dialog.Description className="mt-1 text-sm text-slate-400">
                  仅记录今天曾完整展示过的推荐谱面，共 {recommendationHistory.length} 张。
                </Dialog.Description>
              </div>
              <Dialog.Close aria-label="关闭今日推荐历史" className="text-slate-500 transition hover:text-white">
                <X className="size-5" />
              </Dialog.Close>
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto p-5">
              {recommendationHistory.length ? (
                <div className="space-y-2">
                  {recommendationHistory.map(({ displayed_at, result }) => (
                    <button
                      className="flex w-full items-center gap-4 rounded-xl border border-white/[0.06] bg-black/10 px-4 py-3 text-left transition hover:border-cyan-300/20 hover:bg-white/[0.04]"
                      key={result.beatmap_id}
                      onClick={() => {
                        setHistoryOpen(false);
                        if (result.ruleset === "osu") openOnlineBeatmap(result);
                      }}
                      type="button"
                    >
                      <div className="min-w-0 flex-1">
                        <p className="truncate text-sm font-medium text-slate-100">{result.artist} - {result.title}</p>
                        <p className="mt-1 truncate text-xs text-slate-500">[{result.version}] · {result.creator} · {result.ruleset === "osu" && result.star_rating != null ? `${result.star_rating.toFixed(2)}★` : "星数未知"}</p>
                      </div>
                      <time className="shrink-0 text-xs text-slate-500">{new Intl.DateTimeFormat("zh-CN", { hour: "2-digit", minute: "2-digit", timeZone: APP_TIME_ZONE }).format(new Date(displayed_at))}</time>
                    </button>
                  ))}
                </div>
              ) : (
                <EmptyState title="今天还没有完整推荐记录" description="当一页推荐谱面完整展示后，会自动出现在这里。" />
              )}
            </div>
          </Dialog.Content>
        </Dialog.Portal>
      </Dialog.Root>
      <PageHeader
        title="相似谱面"
        description="以谱面难度特征为参照，从你选择的本地私有索引中寻找相近谱面。索引及查询内容不会上传。"
        actions={<div className="flex items-center gap-2"><Button onClick={() => navigate("/local/maps")} size="sm" variant="secondary"><MapIcon className="size-3.5" />前往本地谱面</Button><InfoTip text="可在本地谱面中展开任一谱面集，并针对其中每个难度单独选择“查找相似”。" /></div>}
      />

      {configurationError ? (
        <div className="mb-5 rounded-xl border border-rose-300/20 bg-rose-300/10 px-4 py-3 text-sm text-rose-100">
          {configurationError}
        </div>
      ) : null}

      {downloadNotice ? (
        <div className="mb-5 rounded-xl border border-emerald-300/20 bg-emerald-300/10 px-4 py-3 text-sm text-emerald-100">
          {downloadNotice}
        </div>
      ) : null}

      {status.state !== "ready" ? (
        <IndexUnavailable
          status={status}
          busy={configuring || statusQuery.isFetching}
          onChoose={() => void chooseIndexDirectory()}
          onRetry={() => void statusQuery.refetch()}
        />
      ) : (
        <>
          <Card className="mb-4 flex items-center justify-between gap-4 border-white/[0.055] bg-black/[0.06] px-4 py-2.5">
            <div className="min-w-0 text-xs text-slate-500">
              <span className="mr-2 inline-flex items-center gap-1.5 text-slate-400"><span className="size-1.5 rounded-full bg-emerald-400/70" />索引已就绪</span>
              <span>
                {status.record_count == null
                  ? "已通过本机校验"
                  : `已从本机读取 ${status.record_count.toLocaleString()} 条记录`}
                {status.analyzer_version == null
                  ? ""
                  : ` · Analyzer v${status.analyzer_version}`}
                {` · ${formatDataCutoff(status.data_cutoff_at)}`}
                {status.data_cutoff_at == null ? "" : "（UTC，非实时数据库）"}
              </span>
            </div>
            <div className="flex gap-2">
              <Button
                type="button"
                size="sm"
                variant="ghost"
                onClick={() => void statusQuery.refetch()}
                disabled={statusQuery.isFetching}
              >
                <RefreshCw size={14} aria-hidden="true" />
                重新校验
              </Button>
              <Button
                type="button"
                size="sm"
                variant="ghost"
                onClick={() => void chooseIndexDirectory()}
                disabled={configuring}
              >
                <FolderOpen size={14} aria-hidden="true" />
                更换目录
              </Button>
            </div>
          </Card>

          <Card className="mb-5 p-5">
          <div className="mb-5 border-b border-white/[0.07] pb-5">
            <div className="mb-3">
              <span className="text-[10px] font-bold uppercase tracking-[0.18em] text-[var(--theme-primary)]">为你推荐</span>
              <p className="mt-1 text-xs text-slate-400">从最近通过或 BP 前 50 张谱面出发，推荐最多 50 张最接近的谱面。</p>
            </div>
            <div className="flex flex-wrap gap-2">
              <Button
                type="button"
                variant="primary"
                disabled={similarityQuery.isPending || similarityRecommendation.isPending}
                loading={similarityRecommendation.isPending && similarityRecommendation.variables?.kind === "recent"}
                onClick={() => recommend("recent")}
              >
                <History size={16} aria-hidden="true" />
                根据最近游玩推荐
              </Button>
              <Button
                type="button"
                disabled={similarityQuery.isPending || similarityRecommendation.isPending}
                loading={similarityRecommendation.isPending && similarityRecommendation.variables?.kind === "best"}
                onClick={() => recommend("best")}
              >
                <Trophy size={16} aria-hidden="true" />
                根据你的 BP 推荐
              </Button>
              <Button
                type="button"
                variant="ghost"
                onClick={() => {
                  setRecommendationHistory(getTodayRecommendationHistory("osu"));
                  setHistoryOpen(true);
                }}
              >
                <History size={16} aria-hidden="true" />
                今日推荐历史
              </Button>
            </div>
          </div>
          <form onSubmit={submit}>
            <div className="mb-5 inline-flex rounded-lg border border-white/[0.08] bg-black/15 p-1" role="tablist" aria-label="参考谱面输入方式">
              <Button
                type="button"
                role="tab"
                aria-selected={request.source.kind === "beatmap_id"}
                size="sm"
                variant={request.source.kind === "beatmap_id" ? "primary" : "ghost"}
                onClick={() => switchSource("beatmap_id")}
              >
                ID / 链接
              </Button>
              <Button
                type="button"
                role="tab"
                aria-selected={request.source.kind === "local_file"}
                size="sm"
                variant={request.source.kind === "local_file" ? "primary" : "ghost"}
                onClick={() => switchSource("local_file")}
              >
                本地 .osu
              </Button>
            </div>

            <div className="flex items-end gap-3">
              <label className="min-w-0 flex-1 text-xs text-slate-400">
                <span className="mb-1.5 block">
                  {request.source.kind === "beatmap_id"
                    ? "Beatmap ID 或 osu! 链接"
                    : "osu!standard 谱面文件"}
                </span>
                <input
                  className="opp-input"
                  value={
                    request.source.kind === "beatmap_id"
                      ? request.source.value
                      : request.source.path
                  }
                  placeholder={
                    request.source.kind === "beatmap_id"
                      ? "例如 1234567 或 https://osu.ppy.sh/beatmaps/1234567"
                      : "选择一个不超过 16 MiB 的 .osu 文件"
                  }
                  readOnly={request.source.kind === "local_file"}
                  onChange={(event) => {
                    const value = event.target.value;
                    setRequest((current) => ({
                      ...current,
                      source: { kind: "beatmap_id", value },
                    }));
                  }}
                />
              </label>
              {request.source.kind === "local_file" ? (
                <Button type="button" onClick={() => void chooseOsuFile()}>
                  <Upload size={16} aria-hidden="true" />
                  选择文件
                </Button>
              ) : null}
              <Button
                variant="primary"
                type="submit"
                disabled={
                  !(request.source.kind === "beatmap_id"
                    ? request.source.value
                    : request.source.path
                  ).trim() || similarityQuery.isPending || similarityRecommendation.isPending
                }
              >
                <Search size={16} aria-hidden="true" />
                {similarityQuery.isPending ? "检索中…" : "查找相似谱面"}
              </Button>
            </div>

            {preferences.advanced_enabled ? <Button
              className="mt-3"
              size="sm"
              variant="ghost"
              type="button"
              aria-expanded={advancedOpen}
              onClick={() => setAdvancedOpen((open) => !open)}
            >
              {advancedOpen ? "收起高级参数" : "展开高级参数"}
            </Button> : null}

            {preferences.advanced_enabled && advancedOpen ? (
              <SimilarityAdvancedPanel request={{ ...request, weighting: effectiveWeighting }} preferences={preferences} supportsDynamicWeighting={status.supports_dynamic_weighting} onChange={changeAdvancedRequest} />
            ) : null}
          </form>
          </Card>

          <SimilarityFilterSliders request={request} onChange={setRequest} />

          {similarityQuery.error || similarityRecommendation.error ? (
            <div className="mb-5 rounded-xl border border-rose-300/20 bg-rose-300/10 px-4 py-3 text-sm text-rose-100">
              {errorMessage(similarityQuery.error ?? similarityRecommendation.error)}
            </div>
          ) : null}

          {response || recommendationResponse ? (
            <section className="grid items-start gap-5 xl:grid-cols-[minmax(0,1fr)_380px]">
              <div className="min-w-0">
                {recommendationResponse ? (
                  <Card className="mb-5 p-5">
                    <span className="text-[10px] font-bold uppercase tracking-[0.18em] text-[var(--theme-primary)]">个性化推荐</span>
                    <h2 className="mt-2 text-lg font-semibold text-white">
                      {recommendationResponse.kind === "recent" ? "根据最近游玩生成" : "根据你的 BP 生成"}
                    </h2>
                    <p className="mt-1 text-sm text-slate-400">
                      已使用 {recommendationResponse.seed_count} 张参考谱面
                      {recommendationResponse.skipped_seed_count
                        ? `，跳过 ${recommendationResponse.skipped_seed_count} 张无法读取的谱面`
                        : ""}
                    </p>
                    {recommendationCompleting ? <p className="mt-3 flex items-center gap-2 text-xs text-[var(--theme-primary-light)]"><LoaderCircle className="size-3.5 animate-spin" />已优先展示 5 首推荐，正在后台完善更多结果</p> : null}
                  </Card>
                ) : response ? (
                  <Card className="similarity-reference-summary mb-4 grid items-center gap-3 px-5 py-4 sm:grid-cols-[minmax(0,1fr)_210px]">
                    <div>
                      <span className="text-[10px] font-bold uppercase tracking-[0.18em] text-[var(--theme-primary)]">参考谱面</span>
                      <h2 className="mt-2 text-lg font-semibold text-white">{response.target.version || "本地谱面"}</h2>
                      <p className="mt-1 text-sm text-slate-400">
                        {response.target.artist} — {response.target.title}
                      </p>
                      <div className="mt-3 flex flex-wrap gap-x-3 gap-y-1 text-xs text-slate-400">
                        <span>AR {formatSimilarityMetric(response.target.base.ar, 1)}</span>
                        <span>星数 {formatSimilarityMetric(response.target.star_rating, 2)}★</span>
                        <span>BPM {formatSimilarityMetric(response.target.base.bpm, 0)}</span>
                        <span>长度 {formatSimilarityMetric(response.target.base.length_seconds, 0)}s</span>
                      </div>
                    </div>
                    <SimilarityRadar compact target={response.target.difficulty} />
                  </Card>
                ) : null}

                <div className="mb-3 flex items-end justify-between gap-3">
                  <div>
                    <span className="text-[10px] font-bold uppercase tracking-[0.18em] text-slate-500">推荐结果</span>
                    <h2 className="mt-1 text-base font-semibold text-white">{filteredResults.length} 个相似谱面集</h2>
                  </div>
                  <div className="flex flex-wrap items-center justify-end gap-2">
                    <span className="text-sm text-slate-500">第 {activeResultBatch + 1} / {resultBatchCount} 批</span>
                    {filteredResults.length > resultsPerPage ? <Button disabled={quickDownloadId !== null} onClick={showNextBatch} size="sm"><RefreshCw className="size-3.5" />换一批</Button> : null}
                    <Button disabled={!visibleResults.length || quickDownloadId !== null} loading={quickDownloadId === -1} onClick={() => void downloadResults(visibleResults)} size="sm" variant="primary"><Download className="size-3.5" />下载本批</Button>
                  </div>
                </div>

                {filteredResults.length ? (
                  <div className="space-y-3">
                    {visibleResults.map((result) => (
                      <SimilarityResultCard
                        key={result.beatmap_id}
                        result={result}
                        recommendedBy={recommendationResponse?.results.find((item) => item.beatmap_id === result.beatmap_id)?.recommended_by}
                        selected={selected?.beatmap_id === result.beatmap_id}
                        onSelect={() => setSelectedResultId(result.beatmap_id)}
                        onDownload={() => void downloadResult(result)}
                        onAddToCollection={() => openCollectionDialog([{ beatmap_id: result.beatmap_id, beatmapset_id: result.beatmapset_id, checksum: null, ruleset: result.ruleset, difficulty_name: result.version, title: result.title, artist: result.artist, creator: result.creator }])}
                        downloading={quickDownloadId === result.beatmap_id}
                        downloadDisabled={quickDownloadId !== null}
                        onOpen={() => openOnlineBeatmap(result)}
                        onPreview={() => void togglePreview(result)}
                        playing={playingId === result.beatmap_id}
                        previewLoading={previewLoadingId === result.beatmap_id}
                      />
                    ))}
                  </div>
                ) : (
                  <EmptyState
                    title="没有符合当前条件的谱面"
                    description="可以放宽 AR、BPM 范围，或调整高级权重后重试。"
                  />
                )}
              </div>

              {selected && comparisonTarget ? (
                <SimilarityComparisonPanel
                  selected={selected}
                  target={comparisonTarget}
                  recommendedBy={recommendedBy}
                  dynamicProfile={selectedDynamicProfile}
                  onOpen={() => openOnlineBeatmap(selected)}
                />
              ) : null}
            </section>
          ) : null}
        </>
      )}
    </>
  );
}

export function SimilarBeatmapsPage() {
  const { ruleset, setRuleset } = useMode();
  const [searchParams] = useSearchParams();
  const launch = parseSimilarityLaunch(searchParams);
  const launchKey = launch ? searchParams.toString() : null;
  const observedLaunchKey = useRef<string | null>(launchKey);
  const [pendingLaunchRuleset, setPendingLaunchRuleset] = useState<Ruleset | null>(() => launch?.ruleset ?? null);
  const pageRuleset = pendingLaunchRuleset ?? ruleset;

  useEffect(() => {
    if (launchKey === null) {
      observedLaunchKey.current = null;
      return;
    }
    if (observedLaunchKey.current === launchKey) return;
    observedLaunchKey.current = launchKey;
    setPendingLaunchRuleset(launch?.ruleset ?? "osu");
  }, [launch?.ruleset, launchKey]);

  useEffect(() => {
    if (pendingLaunchRuleset === null) return;
    const frame = window.requestAnimationFrame(() => {
      if (pendingLaunchRuleset !== ruleset) setRuleset(pendingLaunchRuleset);
      setPendingLaunchRuleset(null);
    });
    return () => window.cancelAnimationFrame(frame);
  }, [pendingLaunchRuleset, ruleset, setRuleset]);

  if (pageRuleset === "mania") return <ManiaSimilarBeatmapsPage />;
  if (pageRuleset === "osu") return <StandardSimilarBeatmapsPage />;

  return (
    <>
      <PageHeader
        title="相似谱面"
        description="相似谱面目前支持 osu!standard 与 osu!mania。"
      />
      <EmptyState
        title={`${pageRuleset === "taiko" ? "osu!taiko" : "osu!catch"} 暂不支持相似谱面`}
        description="请在顶部全局模式中切换到 osu!standard 或 osu!mania。"
      />
    </>
  );
}
