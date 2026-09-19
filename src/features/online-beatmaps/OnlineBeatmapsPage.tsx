import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ArrowLeft, Home, Music2 } from "lucide-react";
import { useReducedMotion } from "motion/react";
import { useQuery } from "@tanstack/react-query";
import { useLocation, useNavigate, useSearchParams } from "react-router-dom";
import { useMode } from "../../app/ModeContext";
import { desktopApi } from "../../shared/lib/tauri";
import { errorMessage } from "../../shared/lib/format";
import type { OnlineBeatmapSearchQuery, OnlineBeatmapset, Ruleset } from "../../shared/types/osu";
import { useOnlineBeatmapsetDetail, useOnlineBeatmapsets, useTrendingBeatmapsets } from "./api";
import { BeatmapsetDetailDialog } from "./BeatmapsetDetailDialog";
import { parseOnlineBeatmapDeepLink } from "./deepLink";
import { createDefaultSearchQuery, normalizePreviewUrl } from "./filters";
import { similarityRouteForBeatmap } from "../similar-beatmaps/navigation";
import { openCollectionDialog } from "../collections/events";
import { useSettings } from "../settings/api";
import { BeatmapPreviewCard } from "../tools/ToolsPage";
import { StageBackground } from "../../shared/components/StageBackground";
import { OnlineStageToolbar } from "./OnlineStageToolbar";
import { OnlineStageSong } from "./OnlineStageSong";
import { OnlineHome } from "./OnlineHome";
import { DownloadDrawer } from "./DownloadDrawer";
import { OnlineResults } from "./OnlineResults";
import { downloadSession } from "./downloadSession";
import { useOnlineDownload } from "./useOnlineDownload";
import { onlineResultsTitle, pickRandomBeatmapset, searchSameTitle, uniqueBeatmapsets, filterChips, type OnlineView, type StageOrigin } from "./stageModel";
import { useOnlineStageArtwork } from "./useOnlineStageArtwork";
import { useOnlineControls } from "./useOnlineControls";
import "./onlineStage.css";

function collectionCandidates(set: OnlineBeatmapset) {
  return (set.beatmaps ?? []).map((map) => ({ beatmap_id: map.id, beatmapset_id: set.id, checksum: map.checksum ?? null, ruleset: map.mode, difficulty_name: map.version, title: set.title_unicode ?? set.title, artist: set.artist_unicode ?? set.artist, creator: set.creator }));
}
function beatmapsetIdFromLookup(value: Record<string, unknown>) {
  const nested = value.beatmapset;
  const id = Number(value.beatmapset_id ?? value.beatmap_set_id ?? value.set_id ?? (nested && typeof nested === "object" ? (nested as Record<string, unknown>).id : null));
  return Number.isSafeInteger(id) && id > 0 ? id : null;
}

function OnlineBeatmapsClient({ ruleset, linkedQuery }: { ruleset: Ruleset; linkedQuery: string }) {
  const navigate = useNavigate();
  const location = useLocation();
  const [searchParams, setSearchParams] = useSearchParams();
  const deepLink = parseOnlineBeatmapDeepLink(searchParams);
  const linkedStage = deepLink.beatmapsetId !== null || deepLink.beatmapId !== null;
  const [view, setView] = useState<OnlineView>(linkedStage ? "stage" : linkedQuery ? "results" : "home");
  const [origin, setOrigin] = useState<StageOrigin>(linkedStage ? "external" : "results");
  const [hasSearched, setHasSearched] = useState(!!linkedQuery);
  const [query, setQuery] = useState<OnlineBeatmapSearchQuery>(() => ({ ...createDefaultSearchQuery(ruleset), query: linkedQuery, sort: linkedQuery ? "relevance_desc" : "ranked_desc" }));
  const [text, setText] = useState(linkedQuery);
  const [selection, setSelection] = useState<{ id: number; fallback?: OnlineBeatmapset; beatmapId?: number | null } | null>(deepLink.beatmapsetId ? { id: deepLink.beatmapsetId, beatmapId: deepLink.beatmapId } : null);
  const linkKey = linkedStage ? `${deepLink.beatmapsetId}:${deepLink.beatmapId}` : null;
  const [previousLink, setPreviousLink] = useState(linkKey);
  if (previousLink !== linkKey) {
    setPreviousLink(linkKey);
    if (linkedStage) {
      setView("stage"); setOrigin("external");
      setSelection(deepLink.beatmapsetId ? { id: deepLink.beatmapsetId, beatmapId: deepLink.beatmapId } : null);
    }
  }
  const lookup = useQuery({ queryKey: ["online-beatmap", deepLink.beatmapId], queryFn: async () => {
    const result = await desktopApi.getOnlineBeatmap(deepLink.beatmapId!);
    if (!beatmapsetIdFromLookup(result)) throw new Error("未找到该难度所属的谱面集。");
    return result;
  }, enabled: view === "stage" && origin === "external" && selection === null && deepLink.beatmapId !== null, retry: false, staleTime: 5 * 60_000 });
  const [manualDetailId, setManualDetailId] = useState<number | null>(null);
  const [playingId, setPlayingId] = useState<number | null>(null);
  const [previewSet, setPreviewSet] = useState<OnlineBeatmapset | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [collecting, setCollecting] = useState(false);
  const collectLock = useRef(false);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const homeScroll = useRef(0);
  const workspace = useRef<HTMLElement>(null);
  const sourceIsTrend = origin === "home" || (origin === "external" && !hasSearched);
  const search = useOnlineBeatmapsets(query, hasSearched && view !== "home" && !(view === "stage" && sourceIsTrend));
  const trend = useTrendingBeatmapsets(ruleset);
  const trending = trend.data ?? [];
  const settings = useSettings();
  const systemReduced = useReducedMotion();
  const reduced = !!settings.data?.reduce_motion || !!systemReduced;
  const { state: downloads, start: startDownload } = useOnlineDownload();
  const items = useMemo(() => uniqueBeatmapsets(search.data?.pages.flatMap((page) => page.beatmapsets ?? []) ?? []), [search.data]);
  const selectedId = selection?.id ?? beatmapsetIdFromLookup(lookup.data ?? {});
  const detailQuery = useOnlineBeatmapsetDetail(selectedId);
  const selected = selection?.fallback ?? items.find((item) => item.id === selectedId) ?? trending.find((item) => item.id === selectedId);
  const set = detailQuery.data?.id === selectedId ? detailQuery.data : selected;
  const volume = settings.data?.preview_volume ?? 65;
  const artwork = useOnlineStageArtwork(set ?? selection?.fallback ?? trending[0], selection !== null || view === "stage");
  useEffect(() => () => { audioRef.current?.pause(); audioRef.current = null; }, []);
  useEffect(() => { audioRef.current?.pause(); }, [linkKey]);
  useEffect(() => { if (audioRef.current) audioRef.current.volume = volume / 100; }, [volume]);
  const preview = (item: OnlineBeatmapset) => {
    const source = normalizePreviewUrl(item.preview_url);
    if (!source) return;
    if (playingId === item.id && audioRef.current) { audioRef.current.pause(); return; }
    audioRef.current?.pause();
    const audio = new Audio(source); audio.volume = volume / 100;
    audioRef.current = audio; setPlayingId(item.id);
    audio.onpause = audio.onended = () => { if (audioRef.current === audio) setPlayingId(null); };
    audio.onerror = () => { if (audioRef.current === audio) { setPlayingId(null); setNotice("试听加载失败，请稍后重试。"); } };
    void audio.play().catch(() => { if (audioRef.current === audio) { setPlayingId(null); setNotice("无法播放试听，请稍后重试。"); } });
  };
  const clearDeepLink = () => {
    if (searchParams.has("beatmapset") || searchParams.has("beatmap")) {
      const next = new URLSearchParams(searchParams); next.delete("beatmapset"); next.delete("beatmap"); setSearchParams(next, { replace: true });
    }
  };
  const apply = (next: OnlineBeatmapSearchQuery) => {
    audioRef.current?.pause(); setNotice(null); clearDeepLink();
    setText(next.query); setQuery({ ...next, cursor_string: null }); setHasSearched(true); setOrigin("results"); setView("results");
  };
  const choose = (item: OnlineBeatmapset, from: StageOrigin) => {
    if (playingId !== item.id) audioRef.current?.pause();
    clearDeepLink(); setSelection({ id: item.id, fallback: item }); setOrigin(from); setView("stage"); setNotice(null);
  };
  const goHome = () => { if (set) setSelection({ id: set.id, fallback: set }); audioRef.current?.pause(); clearDeepLink(); setView("home"); setNotice(null); };
  const goBack = () => {
    if (view === "results") { goHome(); return; }
    if (view === "home") return;
    audioRef.current?.pause();
    const returnTo = location.state?.returnTo;
    if (origin === "external" && typeof returnTo === "string" && returnTo.startsWith("/online/similar")) { navigate(returnTo); return; }
    clearDeepLink(); setView(origin === "results" || (origin === "external" && hasSearched) ? "results" : "home");
  };
  const add = (candidates: OnlineBeatmapset[]) => {
    const result = downloadSession.add(candidates);
    setNotice(`已加入 ${result.added} 首，重复 ${result.duplicates} 首，禁止下载 ${result.blocked} 首。`);
  };
  const collect = async (limit: number) => {
    if (collectLock.current) return;
    collectLock.current = true; setCollecting(true); setNotice(null);
    try {
      const result = await desktopApi.collectOnlineBeatmapsets({ ...query, cursor_string: null }, limit);
      const added = downloadSession.add(result.items);
      setNotice(`按发起时的筛选已加入 ${added.added} 首，重复 ${added.duplicates} 首，禁止下载 ${added.blocked} 首。${result.truncated ? `结果较多，仅收集前 ${limit} 首。` : ""}`);
    } catch (error) { setNotice(errorMessage(error)); }
    finally { collectLock.current = false; setCollecting(false); }
  };
  const { isFetching, hasNextPage, fetchNextPage } = search;
  const loadMore = useCallback(() => { if (!isFetching && hasNextPage) void fetchNextPage(); }, [isFetching, hasNextPage, fetchNextPage]);
  const pool = view === "stage" && sourceIsTrend ? trending : items;
  const queuedIds = new Set(downloads.queue.map((item) => item.id));
  const chips = filterChips(query);
  const backLabel = view === "results" ? "返回搜索首页" : origin === "results" || (origin === "external" && hasSearched) ? "返回搜索结果" : origin === "external" && location.state?.returnTo ? "返回来源" : "返回热门";
  useOnlineControls({ workspaceRef: workspace, view, onBack: goBack, onHome: goHome,
    onStep: (direction) => { const index = pool.findIndex((item) => item.id === selectedId); const next = pool[index + direction]; if (next) choose(next, origin); },
    onPreview: () => { if (set) preview(set); },
  });
  return <>
    <section ref={workspace} className="online-stage online-search-engine" data-view={view} style={artwork.style} aria-label="在线谱面工作区" data-reduce-motion={reduced || undefined}>
      <StageBackground source={artwork.source} reduceMotion={reduced} />
      <nav className="online-engine-nav" aria-label="在线谱面导航">
        <span className="online-location">在线谱面{view === "stage" ? " / 舞台" : view === "results" ? " / 搜索" : ""}</span>
        <DownloadDrawer />
      </nav>
      <header className="online-stage-toolbar">
        <div className="online-home-intro" aria-hidden={view !== "home"} inert={view !== "home"}><div><div className="online-home-title"><Music2 /><h1>OPP Beatmaps</h1><p>寻找下一首想玩的谱面</p></div></div></div>
        <OnlineStageToolbar query={query} ruleset={ruleset} items={view === "home" ? trending : pool} text={text} onTextChange={setText} onApply={apply} onClear={() => {
          setText(""); const next = { ...query, query: "", sort: query.sort === "relevance_desc" ? "ranked_desc" : query.sort };
          if (view === "home") setQuery(next); else apply(next);
        }} />
        {view !== "home" && chips.length ? <div className="online-filter-chips online-quiet-scroll" aria-label="已应用的筛选条件">{chips.map((chip) => <button key={chip.key} aria-label={`移除条件 ${chip.label}`} onClick={() => apply({ ...query, ...chip.clear })}>{chip.label}<span aria-hidden="true">×</span></button>)}</div> : null}
      </header>
      {notice || downloads.error ? <p className="online-notice" role="status">{notice || downloads.error}</p> : null}

        {view === "home" ? <div key="home" className="online-home-scene">
          <OnlineHome scrollPositionRef={homeScroll} items={trending} loading={trend.isPending} error={trend.error ? errorMessage(trend.error) : null} busy={downloads.busy} playingId={playingId} queuedIds={queuedIds} onChoose={(item) => choose(item, "home")} onPreview={preview} onDownload={(item) => void startDownload([item])} onRetry={() => void trend.refetch()} />
        </div> : <div key="browser" className="online-browser">

            {view === "stage" ? <main className="online-stage-focus">
              {set ? <OnlineStageSong key={`${set.id}:${selection?.beatmapId ?? (origin === "external" ? deepLink.beatmapId : "")}`} initialBeatmapId={selection?.beatmapId ?? (origin === "external" ? deepLink.beatmapId : null)} set={set} ruleset={query.ruleset} playing={playingId === set.id} busy={downloads.busy} loading={detailQuery.isFetching} onPreview={() => preview(set)} onDownload={() => void startDownload([set])} onDetails={() => setManualDetailId(set.id)} onCollect={() => openCollectionDialog(collectionCandidates(set))} onVisualPreview={() => setPreviewSet(set)} onSimilar={(id, mode) => navigate(similarityRouteForBeatmap(id, mode))} onSearchTitle={() => apply(searchSameTitle(query, set))} onDifficultyWebsite={(id, mode) => { void desktopApi.openExternal(`https://osu.ppy.sh/beatmapsets/${set.id}#${mode}/${id}`).catch((error) => setNotice(errorMessage(error))); }} onWebsite={() => { void desktopApi.openExternal(`https://osu.ppy.sh/beatmapsets/${set.id}`).catch((error) => setNotice(errorMessage(error))); }} /> : <div className="online-stage-loading" role="status">{lookup.error || detailQuery.error ? "谱面加载失败" : "正在读取谱面…"}</div>}
              {lookup.error || detailQuery.error ? <p className="online-notice" role="alert">{errorMessage(lookup.error || detailQuery.error)}<button onClick={() => { if (lookup.error) void lookup.refetch(); else void detailQuery.refetch(); }}>重试详情</button></p> : null}
            </main> : null}

          <div className="online-result-pane">
            <OnlineResults key={view === "stage" && sourceIsTrend ? "trend" : JSON.stringify(query)} title={view === "stage" && sourceIsTrend ? "近期热门" : onlineResultsTitle(query)} playingId={playingId} onPreview={preview} items={pool} selectedId={selectedId ?? undefined} queuedIds={queuedIds} total={view === "stage" && sourceIsTrend ? trending.length : search.data?.pages[0]?.total ?? null} sort={view === "stage" && sourceIsTrend ? "favourites_desc" : query.sort} variant={view === "stage" ? "sidebar" : "grid"} busy={downloads.busy} loading={view === "stage" && sourceIsTrend ? trend.isPending : search.isFetching} error={(view === "stage" && sourceIsTrend ? trend.error : search.error) ? errorMessage(view === "stage" && sourceIsTrend ? trend.error : search.error) : null} hasMore={!(view === "stage" && sourceIsTrend) && !!search.hasNextPage} onSort={(sort) => apply({ ...query, sort })} onRandom={() => { const item = pickRandomBeatmapset(pool, selectedId); if (item) choose(item, view === "stage" ? origin : "results"); }} onChoose={(item) => choose(item, view === "stage" ? origin : "results")} onDownload={(item) => void startDownload([item])} onLoadMore={loadMore} onRetry={() => { if (view === "stage" && sourceIsTrend) void trend.refetch(); else if (search.isFetchNextPageError) void search.fetchNextPage(); else void search.refetch(); }} onAdd={add} onCollect={(limit) => { if (view === "stage" && sourceIsTrend) add(trending.slice(0, limit)); else void collect(limit); }} collecting={collecting} />
          </div>
        </div>}
      <nav className="online-quick-nav" aria-label="快速导航">
        {view !== "home" ? <button onClick={goBack} title="Alt + ← 或鼠标后退侧键"><ArrowLeft />{backLabel}</button> : null}
        {view === "stage" ? <button onClick={goHome} aria-label="返回搜索首页" title="Alt + Home"><Home />首页</button> : null}
        <span><kbd>/</kbd> 搜索 <span className="online-navigation-hint"> · <kbd>Alt ←</kbd> / 侧键 返回</span>{view === "stage" ? <> · <kbd>J / K</kbd> 选曲 · <kbd>空格</kbd> 试听</> : <> · <kbd>↑ ↓</kbd> 选曲 · <kbd>Enter</kbd> 打开</>}</span>
      </nav>
    </section>
    <BeatmapsetDetailDialog key={manualDetailId ?? "closed"} beatmapsetId={manualDetailId} fallback={set ?? null} initialBeatmapId={selection?.beatmapId ?? null} onAddToCollection={(item) => openCollectionDialog(collectionCandidates(item))} onClose={() => setManualDetailId(null)} onFindSimilar={(id, mode) => navigate(similarityRouteForBeatmap(id, mode))} onPreview={preview} onVisualPreview={setPreviewSet} playing={manualDetailId !== null && playingId === manualDetailId} />
    {previewSet ? <div className="online-visual-preview fixed inset-0 z-[120] overflow-y-auto bg-black/70 p-5 backdrop-blur-md" onClick={() => setPreviewSet(null)}><div className="mx-auto mt-8 max-w-4xl" onClick={(event) => event.stopPropagation()}><BeatmapPreviewCard embeddedBeatmapset={{ beatmaps: previewSet.beatmaps ?? [] }} /></div></div> : null}
  </>;
}

export function OnlineBeatmapsPage() {
  const { ruleset } = useMode();
  const [params] = useSearchParams();
  const linkedQuery = params.get("query")?.trim() ?? "";
  return <OnlineBeatmapsClient key={`${ruleset}:${linkedQuery}`} ruleset={ruleset} linkedQuery={linkedQuery} />;
}
