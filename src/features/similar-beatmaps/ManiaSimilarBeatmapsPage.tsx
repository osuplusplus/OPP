/* eslint-disable react-refresh/only-export-components */
import { useEffect, useMemo, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { ExternalLink, FolderOpen, RefreshCw } from "lucide-react";
import { useNavigate, useSearchParams } from "react-router-dom";

import { PageHeader } from "../../shared/components/PageHeader";
import { Button, EmptyState } from "../../shared/components/ui";
import { errorMessage } from "../../shared/lib/format";
import { desktopApi } from "../../shared/lib/tauri";
import type { AnySimilarityResult, ManiaGameMod, ManiaKeyCount, ManiaSimilarityQueryRequest, ManiaSimilarityQueryResponse, ManiaSimilarityRecommendationResponse, ManiaSimilarityResult, SimilarityIndexStatus, SimilarityRecommendationKind, SimilaritySource } from "../../shared/types/osu";
import { openCollectionDialog } from "../collections/events";
import { normalizePreviewUrl } from "../online-beatmaps/filters";
import { resolveDefaultDownloadProvider } from "../online-beatmaps/downloadProvider";
import { settingsQueryKey, useSettings } from "../settings/api";
import { similarityIndexStatusKey, similarityRecommendationKey, useSimilarityIndexStatus, useSimilarityQuery, useSimilarityRecommendation } from "./api";
import { createManiaSimilarityRequest } from "./defaults";
import { defaultManiaCandidateFilters, ManiaCandidateFilters, matchesManiaCandidate, type ManiaCandidateFilterValue } from "./ManiaCandidateFilters";
import { onlineBeatmapRouteForSimilarityResult, parseSimilarityLaunch } from "./navigation";
import { excludeTodayRecommendedResults, getFilterTodayRecommended, getTodayRecommendationHistory, getTodayRecommendedBeatmapIds, recordDisplayedRecommendation, setFilterTodayRecommended, type RecommendationHistoryEntry } from "./recommendationHistory";
import { RecommendationHistoryControls, SimilarityHistoryDialog, SimilarityHome, SimilarityMessage, SimilaritySearch, SimilarityStage } from "./SimilarityWorkspace";
import { similarityIndexStateCopy } from "./viewModel";

const KEY_COUNTS = [4, 6, 7] as const;
const MANIA_MODS: ManiaGameMod[] = ["NM", "DT", "HT"];
const resultKey = (result: { beatmap_id: number; game_mod: ManiaGameMod }) => `${result.beatmap_id}:${result.game_mod}`;

interface ManiaSession { request: ManiaSimilarityQueryRequest; response: ManiaSimilarityQueryResponse | null; recommendation: ManiaSimilarityRecommendationResponse | null; selectedKey: string | null; activeKeyCount: ManiaKeyCount; }
let maniaSession: ManiaSession | null = null;
export function resetManiaSimilaritySessionForTests() { maniaSession = null; }

function IndexUnavailable({ status, busy, onChoose, onRetry }: { status: SimilarityIndexStatus; busy: boolean; onChoose: () => void; onRetry: () => void }) {
  const copy = similarityIndexStateCopy[status.state as Exclude<SimilarityIndexStatus["state"], "ready">];
  return <EmptyState action={<div className="flex justify-center gap-2"><Button type="button" variant="primary" onClick={onChoose} disabled={busy}><FolderOpen size={16} />选择 Mania 索引目录</Button><Button type="button" onClick={onRetry} disabled={busy}><RefreshCw size={16} />重新校验</Button></div>} description={`${copy.description}${status.message ? ` ${status.message}` : ""}`} icon={<a aria-label="查看 Mania 索引说明" href="https://github.com/osuplusplus/osu-difficulty-lab/tree/1fa21fa6a5144992df58efe7ce9d96019981fad3" rel="noreferrer" target="_blank"><ExternalLink size={22} /></a>} title={copy.title} />;
}

function mergeRecommendation(current: ManiaSimilarityRecommendationResponse | null, next: ManiaSimilarityRecommendationResponse) {
  if (!current) return next;
  return {
    ...next,
    groups: KEY_COUNTS.map((keyCount) => {
      const oldGroup = current.groups.find((group) => group.key_count === keyCount);
      const newGroup = next.groups.find((group) => group.key_count === keyCount);
      const results = [...(oldGroup?.results ?? [])];
      const known = new Set(results.map(resultKey));
      for (const result of newGroup?.results ?? []) if (!known.has(resultKey(result))) results.push(result);
      return { key_count: keyCount, seed_count: newGroup?.seed_count ?? oldGroup?.seed_count ?? 0, results };
    }),
  };
}

export function ManiaSimilarBeatmapsPage() {
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const queryClient = useQueryClient();
  const settings = useSettings();
  const statusQuery = useSimilarityIndexStatus("mania");
  const similarityQuery = useSimilarityQuery("mania");
  const similarityRecommendation = useSimilarityRecommendation("mania");
  const [request, setRequest] = useState<ManiaSimilarityQueryRequest>(() => maniaSession?.request ?? createManiaSimilarityRequest({ kind: "beatmap_id", value: "" }));
  const [response, setResponse] = useState<ManiaSimilarityQueryResponse | null>(() => maniaSession?.response ?? null);
  const [recommendation, setRecommendation] = useState<ManiaSimilarityRecommendationResponse | null>(() => maniaSession?.recommendation ?? null);
  const [activeKeyCount, setActiveKeyCount] = useState<ManiaKeyCount>(() => maniaSession?.activeKeyCount ?? 4);
  const [selectedKey, setSelectedKey] = useState<string | null>(() => maniaSession?.selectedKey ?? null);
  const [filters, setFilters] = useState<ManiaCandidateFilterValue>({ ...defaultManiaCandidateFilters });
  const [historyOpen, setHistoryOpen] = useState(false);
  const [history, setHistory] = useState<RecommendationHistoryEntry[]>(() => getTodayRecommendationHistory("mania"));
  const [filterToday, setFilterToday] = useState(() => getFilterTodayRecommended("mania"));
  const [configuring, setConfiguring] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [downloadNotice, setDownloadNotice] = useState<string | null>(null);
  const [downloadId, setDownloadId] = useState<number | null>(null);
  const [downloadDirectory, setDownloadDirectory] = useState<string | null>(null);
  const [playingId, setPlayingId] = useState<number | null>(null);
  const [previewLoadingId, setPreviewLoadingId] = useState<number | null>(null);
  const [recommendationCompleting, setRecommendationCompleting] = useState(false);
  const [searchText, setSearchText] = useState("");
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const handledLaunch = useRef<string | null>(null);
  const recommendationRun = useRef(0);
  const previewVolume = settings.data?.preview_volume ?? 65;
  const status = statusQuery.data ?? ({ ruleset: "mania", state: "unconfigured", directory: null, record_count: null, analyzer_version: null, normalization_version: null, algorithm_id: null, data_cutoff_at: null, supports_dynamic_weighting: false, records_by_key_count: null, message: statusQuery.error ? errorMessage(statusQuery.error) : "" } satisfies SimilarityIndexStatus);
  const activeGroup = recommendation?.groups.find((group) => group.key_count === activeKeyCount) ?? null;
  const unfilteredResults = useMemo(() => recommendation ? activeGroup?.results ?? [] : response?.results ?? [], [activeGroup, recommendation, response]);
  const results = useMemo(() => unfilteredResults.filter((result) => matchesManiaCandidate(result, filters)), [filters, unfilteredResults]);
  const selected = results.find((result) => resultKey(result) === selectedKey) ?? results[0] ?? null;
  const stageResult = selected ?? unfilteredResults.find((result) => resultKey(result) === selectedKey) ?? unfilteredResults[0] ?? null;
  const selectedIndex = selected ? results.findIndex((result) => resultKey(result) === resultKey(selected)) : -1;
  const recommendedBy = stageResult && recommendation ? activeGroup?.results.find((result) => resultKey(result) === resultKey(stageResult))?.recommended_by ?? null : null;
  const source = recommendedBy ?? response?.target ?? null;

  useEffect(() => { maniaSession = { request, response, recommendation, selectedKey, activeKeyCount }; }, [activeKeyCount, recommendation, request, response, selectedKey]);
  useEffect(() => () => { audioRef.current?.pause(); }, []);
  useEffect(() => { audioRef.current?.pause(); audioRef.current = null; }, [selected?.beatmap_id, selected?.game_mod]);
  useEffect(() => { if (audioRef.current) audioRef.current.volume = previewVolume / 100; }, [previewVolume]);

  useEffect(() => {
    const launch = parseSimilarityLaunch(searchParams);
    const key = searchParams.toString();
    if (!launch) { handledLaunch.current = null; return; }
    if (launch.ruleset !== "mania" || settings.isLoading || status.state !== "ready" || handledLaunch.current === key) return;
    handledLaunch.current = key;
    void (async () => {
      const source: SimilaritySource = launch.kind === "beatmap_id" ? { kind: "beatmap_id", value: launch.beatmapId } : { kind: "local_file", path: await desktopApi.getLocalBeatmapPath(launch.client, launch.resourceId) };
      runSource(source);
      setSearchParams(new URLSearchParams(), { replace: true });
    })().catch((error) => setNotice(errorMessage(error)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [searchParams, setSearchParams, settings.isLoading, status.state]);

  function resetResultState() {
    recommendationRun.current += 1;
    similarityQuery.reset(); similarityRecommendation.reset();
    setResponse(null); setRecommendation(null); setSelectedKey(null); setRecommendationCompleting(false); setNotice(null);
  }
  function runSource(source: SimilaritySource) {
    resetResultState();
    const next: ManiaSimilarityQueryRequest = { ...request, source };
    setRequest(next);
    similarityQuery.mutate(next, { onSuccess: (value) => { if (value.ruleset !== "mania") return; setResponse(value); setActiveKeyCount(value.target.key_count); } });
  }
  async function chooseFile() { const path = await desktopApi.chooseSimilarityBeatmapFile(); if (path) runSource({ kind: "local_file", path }); }
  function selectTargetMod(targetMod: ManiaGameMod) { setRequest((current) => ({ ...current, target_mod: targetMod, candidate_mods: current.candidate_mods.length > 1 ? current.candidate_mods : [targetMod] })); }
  function setMixedMods(enabled: boolean) { setRequest((current) => ({ ...current, candidate_mods: enabled ? [...MANIA_MODS] : [current.target_mod] })); }

  function recommend(kind: SimilarityRecommendationKind) {
    const run = recommendationRun.current + 1;
    resetResultState(); recommendationRun.current = run;
    const clean = (value: ManiaSimilarityRecommendationResponse): ManiaSimilarityRecommendationResponse => filterToday ? ({ ...value, groups: value.groups.map((group) => ({ ...group, results: excludeTodayRecommendedResults(group.results, "mania") })) }) : value;
    const fullRequest = { ruleset: "mania" as const, kind, result_limit: request.result_limit, excluded_beatmap_ids: filterToday ? [...getTodayRecommendedBeatmapIds("mania")] : [], candidate_mods: request.candidate_mods };
    const fullKey = similarityRecommendationKey(fullRequest);
    const complete = (value: ManiaSimilarityRecommendationResponse) => { if (recommendationRun.current !== run) return; queryClient.setQueryData(fullKey, value); const cleaned = clean(value); setRecommendation((current) => mergeRecommendation(current, cleaned)); setActiveKeyCount((current) => cleaned.groups.some((group) => group.key_count === current && group.results.length) ? current : cleaned.groups.find((group) => group.results.length)?.key_count ?? 4); setRecommendationCompleting(false); };
    const cached = queryClient.getQueryData<ManiaSimilarityRecommendationResponse>(fullKey);
    if (cached) { complete(cached); return; }
    const quickRequest = { ...fullRequest, result_limit: 5, seed_limit: 5 };
    const quickKey = similarityRecommendationKey(quickRequest);
    const finish = () => { if (fullRequest.result_limit <= 5) return; setRecommendationCompleting(true); void desktopApi.recommendSimilarBeatmaps(fullRequest).then((value) => { if (value.ruleset === "mania") complete(value); }).catch((error) => { if (recommendationRun.current === run) { setRecommendationCompleting(false); setNotice(errorMessage(error)); } }); };
    const quickCached = queryClient.getQueryData<ManiaSimilarityRecommendationResponse>(quickKey);
    if (quickCached) { const value = clean(quickCached); setRecommendation(value); setActiveKeyCount(value.groups.find((group) => group.results.length)?.key_count ?? 4); finish(); return; }
    similarityRecommendation.mutate(quickRequest, { onSuccess: (value) => { if (recommendationRun.current !== run || value.ruleset !== "mania") return; queryClient.setQueryData(quickKey, value); const cleaned = clean(value); setRecommendation(cleaned); setActiveKeyCount(cleaned.groups.find((group) => group.results.length)?.key_count ?? 4); finish(); } });
  }

  async function chooseIndex() {
    const directory = await desktopApi.chooseDirectory("选择 osu!mania 相似谱面索引目录", status.directory ?? undefined); if (!directory) return;
    setConfiguring(true); setNotice(null);
    try { const value = await desktopApi.configureSimilarityIndex("mania", directory); queryClient.setQueryData(similarityIndexStatusKey("mania"), value); await queryClient.invalidateQueries({ queryKey: settingsQueryKey }); resetResultState(); }
    catch (error) { setNotice(errorMessage(error)); } finally { setConfiguring(false); }
  }

  async function download(result: ManiaSimilarityResult) {
    if (!result.online_url) return;
    let destination = downloadDirectory ?? settings.data?.beatmap_download_directory ?? "";
    if (!destination) { destination = await desktopApi.chooseBeatmapDownloadDirectory(null) ?? ""; if (!destination) return; setDownloadDirectory(destination); if (settings.data) { const saved = await desktopApi.updateSettings({ ...settings.data, beatmap_download_directory: destination }); queryClient.setQueryData(settingsQueryKey, saved); } }
    setDownloadId(result.beatmap_id); setDownloadNotice(null);
    try { const value = await desktopApi.downloadOnlineBeatmapsets({ destination, provider: resolveDefaultDownloadProvider(settings.data), overwrite: false, include_video: settings.data?.include_video_in_beatmap_downloads ?? true, items: [{ beatmapset_id: result.beatmapset_id, artist: result.artist, title: result.title }] }); setDownloadNotice(value.completed ? `已下载到：${value.destination}` : `下载已处理；保存位置：${value.destination}`); }
    catch (error) { setNotice(errorMessage(error)); } finally { setDownloadId(null); }
  }
  async function togglePreview(result: ManiaSimilarityResult) {
    if (!result.online_url) return;
    if (playingId === result.beatmap_id && audioRef.current) { audioRef.current.pause(); audioRef.current = null; setPlayingId(null); return; }
    setPreviewLoadingId(result.beatmap_id);
    try { const beatmapset = await desktopApi.getOnlineBeatmapset(result.beatmapset_id); const url = normalizePreviewUrl(beatmapset.preview_url); if (!url) throw new Error("该谱面没有可用试听音频"); audioRef.current?.pause(); const audio = new Audio(url); audio.volume = previewVolume / 100; audio.onended = () => setPlayingId(null); audio.onerror = () => setPlayingId(null); audioRef.current = audio; setPlayingId(result.beatmap_id); await audio.play(); }
    catch (error) { setNotice(errorMessage(error)); setPlayingId(null); audioRef.current = null; } finally { setPreviewLoadingId(null); }
  }
  function openOnline(result: ManiaSimilarityResult) { if (!result.online_url) return; maniaSession = { request, response, recommendation, selectedKey: resultKey(result), activeKeyCount }; navigate(onlineBeatmapRouteForSimilarityResult(result), { state: { returnTo: "/online/similar" } }); }

  if (statusQuery.isLoading) return <><PageHeader title="相似谱面" description="正在检查本地 Mania 相似谱面索引。" /><EmptyState title="正在校验 Mania 索引" description="正在以只读方式检查本机配置。" icon={<RefreshCw className="animate-spin" size={22} />} /></>;
  if (status.state !== "ready") return <><PageHeader title="相似谱面" description="从本地私有索引中寻找特征相近的 osu!mania 谱面。" />{notice ? <p className="online-notice" role="alert">{notice}</p> : null}<IndexUnavailable status={status} busy={configuring || statusQuery.isFetching} onChoose={() => void chooseIndex()} onRetry={() => void statusQuery.refetch()} /></>;

  const busy = similarityQuery.isPending || similarityRecommendation.isPending;
  const showHome = !response && !recommendation;
  const modControls = <div className="flex flex-wrap items-center gap-2">{MANIA_MODS.map((gameMod) => <Button key={gameMod} size="sm" type="button" variant={request.target_mod === gameMod ? "primary" : "ghost"} aria-pressed={request.target_mod === gameMod} onClick={() => selectTargetMod(gameMod)}>{gameMod}</Button>)}<label className="flex items-center gap-2 text-xs text-slate-300"><input type="checkbox" aria-label="NM / DT / HT 多 Mod 混池" checked={request.candidate_mods.length > 1} onChange={(event) => setMixedMods(event.target.checked)} />多 Mod</label></div>;
  const keyTabs = recommendation ? <div className="flex gap-1" role="tablist" aria-label="Mania 键数分组">{KEY_COUNTS.map((keyCount) => { const group = recommendation.groups.find((item) => item.key_count === keyCount); return <Button key={keyCount} size="sm" role="tab" aria-selected={activeKeyCount === keyCount} variant={activeKeyCount === keyCount ? "primary" : "ghost"} onClick={() => { setActiveKeyCount(keyCount); setSelectedKey(null); }}>{keyCount}K · {group?.results.length ?? 0}</Button>; })}</div> : null;
  return <>
    <SimilarityHistoryDialog open={historyOpen} title="今日 Mania 推荐历史" entries={history} onClose={() => setHistoryOpen(false)} onChoose={(result: AnySimilarityResult) => { if (result.ruleset === "mania" && result.online_url) { setHistoryOpen(false); openOnline(result); } }} />
    <SimilarityMessage message={notice ?? (similarityQuery.error || similarityRecommendation.error ? errorMessage(similarityQuery.error ?? similarityRecommendation.error) : null)} onClose={() => { setNotice(null); similarityQuery.reset(); similarityRecommendation.reset(); }} />
    <SimilarityMessage message={downloadNotice} tone="status" onClose={() => setDownloadNotice(null)} />
    {showHome ? <><SimilarityHome busy={busy} ruleset="mania" searchValue={searchText} onSearchValueChange={setSearchText} onChoose={runSource} onChooseFile={() => void chooseFile()} onRecommend={recommend} onHistory={() => { setHistory(getTodayRecommendationHistory("mania")); setHistoryOpen(true); }} filterToday={filterToday} onFilterTodayChange={(enabled) => { setFilterToday(enabled); setFilterTodayRecommended("mania", enabled); }} status={<><span>Mania 索引已就绪 · {status.record_count?.toLocaleString() ?? "已校验"} 条记录</span><button type="button" disabled={statusQuery.isFetching} onClick={() => statusQuery.revalidate()}>重新校验</button><button type="button" onClick={() => void chooseIndex()}>更换目录</button></>} /><div className="mx-auto -mt-16 flex max-w-[820px] justify-center">{modControls}</div></> : stageResult && source ? <SimilarityStage
      result={stageResult} source={source} index={selectedIndex} total={results.length || unfilteredResults.length} emptyMessage={results.length ? null : "请重新打开筛选调整条件；来源谱面和当前舞台会保持不变。"} recommendation={Boolean(recommendation)} playing={playingId === stageResult.beatmap_id} previewLoading={previewLoadingId === stageResult.beatmap_id} downloading={downloadId === stageResult.beatmap_id} completing={recommendationCompleting}
      onHome={resetResultState}
      onDisplayed={() => { if (recommendation && selected) recordDisplayedRecommendation(selected, "mania"); }}
      onPrevious={() => setSelectedKey(results[selectedIndex - 1] ? resultKey(results[selectedIndex - 1]) : null)} onNext={() => setSelectedKey(results[selectedIndex + 1] ? resultKey(results[selectedIndex + 1]) : null)} onPreview={() => void togglePreview(stageResult)} onDownload={() => void download(stageResult)}
      onCollect={() => { if (!stageResult.online_url) return; openCollectionDialog([{ beatmap_id: stageResult.beatmap_id, beatmapset_id: stageResult.beatmapset_id, checksum: null, ruleset: stageResult.ruleset, difficulty_name: `${stageResult.version} +${stageResult.game_mod}`, title: stageResult.title, artist: stageResult.artist, creator: stageResult.creator }]); }}
      toolbar={<><SimilaritySearch compact busy={busy} ruleset="mania" value={searchText} onValueChange={setSearchText} onChoose={runSource} onChooseFile={() => void chooseFile()} /><ManiaCandidateFilters value={filters} onChange={setFilters} total={unfilteredResults.length} visible={results.length} />{keyTabs}<RecommendationHistoryControls compact filterToday={filterToday} onFilterTodayChange={(enabled) => { setFilterToday(enabled); setFilterTodayRecommended("mania", enabled); }} onHistory={() => { setHistory(getTodayRecommendationHistory("mania")); setHistoryOpen(true); }} /></>}
      details={<div className="flex flex-wrap items-center gap-2">{modControls}</div>}
    /> : <div className="p-8"><div className="mb-4 flex flex-wrap items-center justify-center gap-3"><SimilaritySearch compact busy={busy} ruleset="mania" onChoose={runSource} onChooseFile={() => void chooseFile()} /><ManiaCandidateFilters value={filters} onChange={setFilters} total={unfilteredResults.length} visible={results.length} />{keyTabs}</div><EmptyState title={`没有符合条件的 ${activeKeyCount}K 候选`} description="可清除候选过滤条件、切换键数，或换一张参考谱面。" /></div>}
  </>;
}
