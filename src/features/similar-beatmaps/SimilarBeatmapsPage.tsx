/* eslint-disable react-refresh/only-export-components */
import { useEffect, useMemo, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { ExternalLink, FolderOpen, RefreshCw } from "lucide-react";
import { useNavigate, useSearchParams } from "react-router-dom";

import { useMode } from "../../app/ModeContext";
import { PageHeader } from "../../shared/components/PageHeader";
import { Button, EmptyState } from "../../shared/components/ui";
import { errorMessage } from "../../shared/lib/format";
import { desktopApi } from "../../shared/lib/tauri";
import type { AnySimilarityResult, OsuSimilarityQueryRequest, OsuSimilarityQueryResponse, OsuSimilarityRecommendationResponse, Ruleset, SimilarityIndexStatus, SimilarityRecommendationKind, SimilarityResult, SimilaritySource } from "../../shared/types/osu";
import { openCollectionDialog } from "../collections/events";
import { LocalLibraryBackdrop } from "../local-analysis/LocalLibraryBackdrop";
import { normalizePreviewUrl } from "../online-beatmaps/filters";
import { resolveDefaultDownloadProvider } from "../online-beatmaps/downloadProvider";
import { settingsQueryKey, useSettings } from "../settings/api";
import { similarityIndexStatusKey, similarityRecommendationKey, useSimilarityIndexStatus, useSimilarityQuery, useSimilarityRecommendation } from "./api";
import { createSimilarityRequest, defaultSimilarityPreferences } from "./defaults";
import { ManiaSimilarBeatmapsPage } from "./ManiaSimilarBeatmapsPage";
import { onlineBeatmapRouteForSimilarityResult, parseSimilarityLaunch } from "./navigation";
import { excludeTodayRecommendedResults, getFilterTodayRecommended, getTodayRecommendationHistory, getTodayRecommendedBeatmapIds, recordDisplayedRecommendation, setFilterTodayRecommended, type RecommendationHistoryEntry } from "./recommendationHistory";
import { SimilarityAdvancedPanel } from "./SimilarityAdvancedPanel";
import { SimilarityFilterSliders } from "./SimilarityFilterSliders";
import { RecommendationHistoryControls, SimilarityHistoryDialog, SimilarityHome, SimilarityMessage, SimilaritySearch, SimilarityStage } from "./SimilarityWorkspace";
import { matchesCandidateFilters, resolveSimilarityWeighting, similarityIndexStateCopy } from "./viewModel";

interface StandardSession { request: OsuSimilarityQueryRequest; response: OsuSimilarityQueryResponse | null; recommendation: OsuSimilarityRecommendationResponse | null; selectedId: number | null; }
let standardSession: StandardSession | null = null;
export function resetStandardSimilaritySessionForTests() { standardSession = null; }

function IndexUnavailable({ status, busy, onChoose, onRetry }: { status: SimilarityIndexStatus; busy: boolean; onChoose: () => void; onRetry: () => void }) {
  const copy = similarityIndexStateCopy[status.state as Exclude<SimilarityIndexStatus["state"], "ready">];
  return <EmptyState action={<div className="flex justify-center gap-2"><Button type="button" variant="primary" onClick={onChoose} disabled={busy}><FolderOpen size={16} />选择索引目录</Button><Button type="button" onClick={onRetry} disabled={busy}><RefreshCw size={16} />重新校验</Button></div>} description={`${copy.description}${status.message ? ` ${status.message}` : ""}`} icon={<a aria-label="下载相似谱面索引" href="https://github.com/osuplusplus/osu-difficulty-lab/releases" rel="noreferrer" target="_blank"><ExternalLink size={22} /></a>} title={copy.title} />;
}

function mergeRecommendation(current: OsuSimilarityRecommendationResponse | null, next: OsuSimilarityRecommendationResponse) {
  if (!current) return next;
  const results = [...current.results];
  const known = new Set(results.map((result) => result.beatmap_id));
  for (const result of next.results) if (!known.has(result.beatmap_id)) results.push(result);
  return { ...next, results };
}

function StandardSimilarBeatmapsPage() {
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const queryClient = useQueryClient();
  const settings = useSettings();
  const statusQuery = useSimilarityIndexStatus("osu");
  const similarityQuery = useSimilarityQuery("osu");
  const similarityRecommendation = useSimilarityRecommendation("osu");
  const [request, setRequest] = useState<OsuSimilarityQueryRequest>(() => standardSession?.request ?? createSimilarityRequest({ kind: "beatmap_id", value: "" }));
  const [response, setResponse] = useState<OsuSimilarityQueryResponse | null>(() => standardSession?.response ?? null);
  const [recommendation, setRecommendation] = useState<OsuSimilarityRecommendationResponse | null>(() => standardSession?.recommendation ?? null);
  const [selectedId, setSelectedId] = useState<number | null>(() => standardSession?.selectedId ?? null);
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [history, setHistory] = useState<RecommendationHistoryEntry[]>(() => getTodayRecommendationHistory("osu"));
  const [filterToday, setFilterToday] = useState(() => getFilterTodayRecommended("osu"));
  const [configuring, setConfiguring] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [downloadNotice, setDownloadNotice] = useState<string | null>(null);
  const [downloadId, setDownloadId] = useState<number | null>(null);
  const [downloadDirectory, setDownloadDirectory] = useState<string | null>(null);
  const [playingId, setPlayingId] = useState<number | null>(null);
  const [previewLoadingId, setPreviewLoadingId] = useState<number | null>(null);
  const [recommendationCompleting, setRecommendationCompleting] = useState(false);
  const [searchText, setSearchText] = useState("");
  const [focusSkill, setFocusSkill] = useState<string | null>(null);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const handledLaunch = useRef<string | null>(null);
  const recommendationRun = useRef(0);
  const preferenceTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const previewVolume = settings.data?.preview_volume ?? 65;
  const preferences = useMemo(() => ({ ...defaultSimilarityPreferences, ...settings.data?.similarity_preferences, advanced_enabled: true, manual_weights: { ...defaultSimilarityPreferences.manual_weights, ...settings.data?.similarity_preferences?.manual_weights } }), [settings.data?.similarity_preferences]);
  const effectiveWeighting = resolveSimilarityWeighting(request, preferences);
  const status = statusQuery.data ?? ({ ruleset: "osu", state: "unconfigured", directory: null, record_count: null, analyzer_version: null, normalization_version: null, algorithm_id: null, data_cutoff_at: null, supports_dynamic_weighting: false, records_by_key_count: null, message: statusQuery.error ? errorMessage(statusQuery.error) : "" } satisfies SimilarityIndexStatus);
  const allResults = useMemo(() => recommendation?.results ?? response?.results ?? [], [recommendation, response]);
  const results = useMemo(() => allResults.filter((result) => matchesCandidateFilters(result.base, result.star_rating, request.filters)), [allResults, request.filters]);
  const selected = results.find((result) => result.beatmap_id === selectedId) ?? results[0] ?? null;
  const stageResult = selected ?? allResults.find((result) => result.beatmap_id === selectedId) ?? allResults[0] ?? null;
  const selectedIndex = selected ? results.findIndex((result) => result.beatmap_id === selected.beatmap_id) : -1;
  const recommendedBy = stageResult ? recommendation?.results.find((result) => result.beatmap_id === stageResult.beatmap_id)?.recommended_by ?? null : null;
  const source = recommendedBy ?? response?.target ?? null;

  useEffect(() => { standardSession = { request, response, recommendation, selectedId }; }, [recommendation, request, response, selectedId]);
  useEffect(() => () => { audioRef.current?.pause(); if (preferenceTimer.current) clearTimeout(preferenceTimer.current); }, []);
  useEffect(() => { audioRef.current?.pause(); audioRef.current = null; }, [selected?.beatmap_id]);
  useEffect(() => { if (audioRef.current) audioRef.current.volume = previewVolume / 100; }, [previewVolume]);

  useEffect(() => {
    const launch = parseSimilarityLaunch(searchParams);
    const key = searchParams.toString();
    if (!launch) { handledLaunch.current = null; return; }
    if (launch.ruleset !== "osu" || settings.isLoading || status.state !== "ready" || handledLaunch.current === key) return;
    handledLaunch.current = key;
    void (async () => {
      const source: SimilaritySource = launch.kind === "beatmap_id" ? { kind: "beatmap_id", value: launch.beatmapId } : { kind: "local_file", path: await desktopApi.getLocalBeatmapPath(launch.client, launch.resourceId) };
      setFocusSkill(launch.focusSkill ?? null);
      runSource(source);
      setSearchParams(new URLSearchParams(), { replace: true });
    })().catch((error) => setNotice(errorMessage(error)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [searchParams, setSearchParams, settings.isLoading, status.state]);

  function changeAdvancedRequest(next: OsuSimilarityQueryRequest) {
    setRequest(next);
    if (!settings.data) return;
    const nextPreferences = next.weighting.mode === "dynamic" ? { ...preferences, mode: "dynamic" as const, lower_sections: next.weighting.lower_sections, upper_sections: next.weighting.upper_sections } : { ...preferences, mode: "manual" as const, manual_weights: { ...next.weighting.difficulty_weights, parameters: next.weighting.parameter_weight } };
    const nextSettings = { ...settings.data, similarity_preferences: nextPreferences };
    queryClient.setQueryData(settingsQueryKey, nextSettings);
    if (preferenceTimer.current) clearTimeout(preferenceTimer.current);
    preferenceTimer.current = setTimeout(() => void desktopApi.updateSettings(nextSettings).then((saved) => queryClient.setQueryData(settingsQueryKey, saved)), 400);
  }

  function resetResultState() {
    recommendationRun.current += 1;
    similarityQuery.reset(); similarityRecommendation.reset();
    setResponse(null); setRecommendation(null); setSelectedId(null); setRecommendationCompleting(false); setNotice(null);
  }
  function runSource(source: SimilaritySource) {
    resetResultState();
    const next: OsuSimilarityQueryRequest = { ...request, source, weighting: effectiveWeighting, filters: { ...request.filters } };
    setRequest(next);
    similarityQuery.mutate(next, { onSuccess: (value) => { if (value.ruleset === "osu") setResponse(value); } });
  }
  async function chooseFile() { const path = await desktopApi.chooseSimilarityBeatmapFile(); if (path) runSource({ kind: "local_file", path }); }

  function recommend(kind: SimilarityRecommendationKind) {
    const run = recommendationRun.current + 1;
    resetResultState(); recommendationRun.current = run;
    const clean = (value: OsuSimilarityRecommendationResponse): OsuSimilarityRecommendationResponse => filterToday ? ({ ...value, results: excludeTodayRecommendedResults(value.results, "osu") }) : value;
    const fullRequest = { ruleset: "osu" as const, kind, weighting: effectiveWeighting, filters: { ...request.filters }, result_limit: request.result_limit, excluded_beatmap_ids: filterToday ? [...getTodayRecommendedBeatmapIds("osu")] : [] };
    const fullKey = similarityRecommendationKey(fullRequest);
    const complete = (value: OsuSimilarityRecommendationResponse) => { if (recommendationRun.current !== run) return; queryClient.setQueryData(fullKey, value); setRecommendation((current) => mergeRecommendation(current, clean(value))); setRecommendationCompleting(false); };
    const cached = queryClient.getQueryData<OsuSimilarityRecommendationResponse>(fullKey);
    if (cached) { complete(cached); return; }
    const quickRequest = { ...fullRequest, result_limit: 5, seed_limit: 5 };
    const quickKey = similarityRecommendationKey(quickRequest);
    const finish = () => { if (fullRequest.result_limit <= 5) return; setRecommendationCompleting(true); void desktopApi.recommendSimilarBeatmaps(fullRequest).then((value) => { if (value.ruleset === "osu") complete(value); }).catch((error) => { if (recommendationRun.current === run) { setRecommendationCompleting(false); setNotice(errorMessage(error)); } }); };
    const quickCached = queryClient.getQueryData<OsuSimilarityRecommendationResponse>(quickKey);
    if (quickCached) { setRecommendation(clean(quickCached)); finish(); return; }
    similarityRecommendation.mutate(quickRequest, { onSuccess: (value) => { if (recommendationRun.current !== run || value.ruleset !== "osu") return; queryClient.setQueryData(quickKey, value); setRecommendation(clean(value)); finish(); } });
  }

  async function chooseIndex() {
    const directory = await desktopApi.chooseDirectory("选择相似谱面索引目录", status.directory ?? undefined); if (!directory) return;
    setConfiguring(true); setNotice(null);
    try { const value = await desktopApi.configureSimilarityIndex("osu", directory); queryClient.setQueryData(similarityIndexStatusKey("osu"), value); await queryClient.invalidateQueries({ queryKey: settingsQueryKey }); resetResultState(); }
    catch (error) { setNotice(errorMessage(error)); } finally { setConfiguring(false); }
  }

  async function download(result: SimilarityResult) {
    let destination = downloadDirectory ?? settings.data?.beatmap_download_directory ?? "";
    if (!destination) { destination = await desktopApi.chooseBeatmapDownloadDirectory(null) ?? ""; if (!destination) return; setDownloadDirectory(destination); if (settings.data) { const saved = await desktopApi.updateSettings({ ...settings.data, beatmap_download_directory: destination }); queryClient.setQueryData(settingsQueryKey, saved); } }
    setDownloadId(result.beatmap_id); setDownloadNotice(null);
    try { const value = await desktopApi.downloadOnlineBeatmapsets({ destination, provider: resolveDefaultDownloadProvider(settings.data), overwrite: false, include_video: settings.data?.include_video_in_beatmap_downloads ?? true, items: [{ beatmapset_id: result.beatmapset_id, artist: result.artist, title: result.title }] }); setDownloadNotice(value.completed ? `已下载到：${value.destination}` : `下载已处理；保存位置：${value.destination}`); }
    catch (error) { setNotice(errorMessage(error)); } finally { setDownloadId(null); }
  }

  async function togglePreview(result: SimilarityResult) {
    if (playingId === result.beatmap_id && audioRef.current) { audioRef.current.pause(); audioRef.current = null; setPlayingId(null); return; }
    setPreviewLoadingId(result.beatmap_id);
    try { const beatmapset = await desktopApi.getOnlineBeatmapset(result.beatmapset_id); const url = normalizePreviewUrl(beatmapset.preview_url); if (!url) throw new Error("该谱面没有可用试听音频"); audioRef.current?.pause(); const audio = new Audio(url); audio.volume = previewVolume / 100; audio.onended = () => setPlayingId(null); audio.onerror = () => setPlayingId(null); audioRef.current = audio; setPlayingId(result.beatmap_id); await audio.play(); }
    catch (error) { setNotice(errorMessage(error)); setPlayingId(null); audioRef.current = null; } finally { setPreviewLoadingId(null); }
  }
  function openOnline(result: SimilarityResult) { standardSession = { request, response, recommendation, selectedId: result.beatmap_id }; navigate(onlineBeatmapRouteForSimilarityResult(result), { state: { returnTo: "/online/similar" } }); }

  if (statusQuery.isLoading) return <><PageHeader title="相似谱面" description="正在检查本地相似谱面索引。" /><EmptyState title="正在校验本地索引" description="正在以只读方式检查本机配置。" icon={<RefreshCw className="animate-spin" size={22} />} /></>;
  if (status.state !== "ready") return <><PageHeader title="相似谱面" description="从本地私有索引中寻找特征相近的 osu!standard 谱面。" />{notice ? <p className="online-notice" role="alert">{notice}</p> : null}<IndexUnavailable status={status} busy={configuring || statusQuery.isFetching} onChoose={() => void chooseIndex()} onRetry={() => void statusQuery.refetch()} /></>;

  const busy = similarityQuery.isPending || similarityRecommendation.isPending;
  const showHome = !response && !recommendation;
  return <>
    <SimilarityHistoryDialog open={historyOpen} title="今日推荐历史" entries={history} onClose={() => setHistoryOpen(false)} onChoose={(result: AnySimilarityResult) => { setHistoryOpen(false); if (result.ruleset === "osu") openOnline(result); }} />
    <SimilarityMessage message={notice ?? (similarityQuery.error || similarityRecommendation.error ? errorMessage(similarityQuery.error ?? similarityRecommendation.error) : null)} onClose={() => { setNotice(null); similarityQuery.reset(); similarityRecommendation.reset(); }} />
    <SimilarityMessage message={downloadNotice} tone="status" onClose={() => setDownloadNotice(null)} />
    {showHome ? <SimilarityHome busy={busy} ruleset="osu" searchValue={searchText} onSearchValueChange={setSearchText} onChoose={runSource} onChooseFile={() => void chooseFile()} onRecommend={recommend} onHistory={() => { setHistory(getTodayRecommendationHistory("osu")); setHistoryOpen(true); }} filterToday={filterToday} onFilterTodayChange={(enabled) => { setFilterToday(enabled); setFilterTodayRecommended("osu", enabled); }} status={<><span>索引已就绪 · {status.record_count?.toLocaleString() ?? "已校验"} 条记录</span><button type="button" disabled={statusQuery.isFetching} onClick={() => statusQuery.revalidate()}>重新校验</button><button type="button" onClick={() => void chooseIndex()}>更换目录</button></>} /> : stageResult && source ? <SimilarityStage
      result={stageResult} source={source} index={selectedIndex} total={results.length || allResults.length} emptyMessage={results.length ? null : "请重新打开筛选调整条件；来源谱面和当前舞台会保持不变。"} recommendation={Boolean(recommendation)} playing={playingId === stageResult.beatmap_id} previewLoading={previewLoadingId === stageResult.beatmap_id} downloading={downloadId === stageResult.beatmap_id} completing={recommendationCompleting}
      onHome={resetResultState}
      onDisplayed={() => { if (recommendation && selected) recordDisplayedRecommendation(selected, "osu"); }}
      onPrevious={() => setSelectedId(results[selectedIndex - 1]?.beatmap_id ?? null)} onNext={() => setSelectedId(results[selectedIndex + 1]?.beatmap_id ?? null)} onPreview={() => void togglePreview(stageResult)} onDownload={() => void download(stageResult)}
      onCollect={() => openCollectionDialog([{ beatmap_id: stageResult.beatmap_id, beatmapset_id: stageResult.beatmapset_id, checksum: null, ruleset: stageResult.ruleset, difficulty_name: stageResult.version, title: stageResult.title, artist: stageResult.artist, creator: stageResult.creator }])}
      toolbar={<><SimilaritySearch compact busy={busy} ruleset="osu" value={searchText} onValueChange={setSearchText} onChoose={runSource} onChooseFile={() => void chooseFile()} /><SimilarityFilterSliders request={request} onChange={setRequest} /><RecommendationHistoryControls compact filterToday={filterToday} onFilterTodayChange={(enabled) => { setFilterToday(enabled); setFilterTodayRecommended("osu", enabled); }} onHistory={() => { setHistory(getTodayRecommendationHistory("osu")); setHistoryOpen(true); }} /></>}
      details={<div className="space-y-2"><Button size="sm" variant="ghost" onClick={() => setAdvancedOpen(!advancedOpen)}>{advancedOpen ? "收起高级参数" : "高级参数"}</Button>{advancedOpen ? <SimilarityAdvancedPanel request={{ ...request, weighting: effectiveWeighting }} preferences={preferences} onChange={changeAdvancedRequest} /> : null}{focusSkill ? <p>技能训练目标：{focusSkill}</p> : null}</div>}
    /> : <div className="p-8"><SimilaritySearch compact busy={busy} ruleset="osu" onChoose={runSource} onChooseFile={() => void chooseFile()} /><EmptyState title="没有符合条件的候选谱面" description="可清除候选过滤条件，或换一张参考谱面。" /></div>}
  </>;
}

export function SimilarBeatmapsPage() {
  const { ruleset, setRuleset } = useMode();
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const launch = parseSimilarityLaunch(searchParams);
  const launchKey = launch ? searchParams.toString() : null;
  const observed = useRef<string | null>(launchKey);
  const [pendingRuleset, setPendingRuleset] = useState<Ruleset | null>(() => launch?.ruleset ?? null);
  const pageRuleset = pendingRuleset ?? ruleset;
  useEffect(() => { if (launchKey === null) { observed.current = null; return; } if (observed.current === launchKey) return; observed.current = launchKey; setPendingRuleset(launch?.ruleset ?? "osu"); }, [launch?.ruleset, launchKey]);
  useEffect(() => { if (pendingRuleset === null) return; const frame = requestAnimationFrame(() => { if (pendingRuleset !== ruleset) setRuleset(pendingRuleset); setPendingRuleset(null); }); return () => cancelAnimationFrame(frame); }, [pendingRuleset, ruleset, setRuleset]);
  return <section className="similarity-page"><LocalLibraryBackdrop />{pageRuleset === "mania" ? <ManiaSimilarBeatmapsPage /> : pageRuleset === "osu" ? <StandardSimilarBeatmapsPage /> : <><PageHeader title="相似谱面" description="相似谱面目前支持 osu!standard 与 osu!mania。" /><EmptyState title={`${pageRuleset === "taiko" ? "osu!taiko" : "osu!catch"} 暂不支持相似谱面`} description="请前往“设置 → 常规 → 游戏模式”，切换到 osu!standard 或 osu!mania。" action={<Button onClick={() => navigate("/settings")} variant="primary">打开设置</Button>} /></>}</section>;
}
