import { useEffect, useMemo, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { CheckSquare2, ChevronDown, DownloadCloud, Music2, SearchX } from "lucide-react";
import { useLocation, useNavigate, useSearchParams } from "react-router-dom";
import { useMode } from "../../app/ModeContext";
import { ErrorPanel } from "../../shared/components/ErrorPanel";
import { PageHeader } from "../../shared/components/PageHeader";
import { Badge, Button, EmptyState, Skeleton } from "../../shared/components/ui";
import { desktopApi } from "../../shared/lib/tauri";
import type { CommandError, OnlineBeatmapSearchQuery, OnlineBeatmapset, Ruleset } from "../../shared/types/osu";
import { useOnlineBeatmapProviderStatus, useOnlineBeatmapsets } from "./api";
import { BeatmapDownloadPanel, beatmapDownloadDirectoryKey } from "./BeatmapDownloadPanel";
import { BeatmapsetCard } from "./BeatmapsetCard";
import { BeatmapsetDetailDialog } from "./BeatmapsetDetailDialog";
import { parseOnlineBeatmapDeepLink } from "./deepLink";
import { createDefaultSearchQuery, normalizePreviewUrl } from "./filters";
import { resolveDefaultDownloadProvider } from "./downloadProvider";
import { OnlineBeatmapFilters } from "./OnlineBeatmapFilters";
import { OnlineBeatmapSortBar } from "./OnlineBeatmapSortBar";
import { similarityRouteForBeatmap } from "../similar-beatmaps/navigation";
import { openCollectionDialog } from "../collections/events";
import { settingsQueryKey, useSettings } from "../settings/api";

function uniqueBeatmapsets(items: OnlineBeatmapset[]) {
  const seen = new Set<number>();
  return items.filter((item) => !seen.has(item.id) && seen.add(item.id));
}

function collectionCandidates(beatmapset: OnlineBeatmapset) {
  return (beatmapset.beatmaps ?? []).map((beatmap) => ({ beatmap_id: beatmap.id, beatmapset_id: beatmapset.id, checksum: beatmap.checksum ?? null, ruleset: beatmap.mode, difficulty_name: beatmap.version, title: beatmapset.title_unicode ?? beatmapset.title, artist: beatmapset.artist_unicode ?? beatmapset.artist, creator: beatmapset.creator }));
}

function beatmapsetIdFromLookup(value: Record<string, unknown>) {
  const direct = value.beatmapset_id ?? value.beatmap_set_id ?? value.set_id;
  const nested = value.beatmapset;
  const candidate = direct ?? (nested && typeof nested === "object" ? (nested as Record<string, unknown>).id : null);
  const id = Number(candidate);
  return Number.isSafeInteger(id) && id > 0 ? id : null;
}

function OnlineBeatmapsClient({ ruleset }: { ruleset: Ruleset }) {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const location = useLocation();
  const [searchParams, setSearchParams] = useSearchParams();
  const deepLink = parseOnlineBeatmapDeepLink(searchParams);
  const linkedQuery = searchParams.get("query")?.trim() ?? "";
  const beatmapLookup = useQuery({
    queryKey: ["online-beatmap", deepLink.beatmapId],
    queryFn: () => desktopApi.getOnlineBeatmap(deepLink.beatmapId!),
    enabled: deepLink.beatmapsetId === null && deepLink.beatmapId !== null,
    retry: false,
    staleTime: 5 * 60_000,
  });
  const [draft, setDraft] = useState<OnlineBeatmapSearchQuery>(() => ({ ...createDefaultSearchQuery(ruleset), query: linkedQuery, title: linkedQuery }));
  const [activeQuery, setActiveQuery] = useState<OnlineBeatmapSearchQuery>(() => ({ ...createDefaultSearchQuery(ruleset), query: linkedQuery, title: linkedQuery }));
  const [queue, setQueue] = useState<Map<number, OnlineBeatmapset>>(() => new Map());
  const [manualDetailId, setManualDetailId] = useState<number | null>(null);
  const detailId = deepLink.beatmapsetId ?? beatmapsetIdFromLookup(beatmapLookup.data ?? {}) ?? manualDetailId;
  const [playingId, setPlayingId] = useState<number | null>(null);
  const [directDownloadId, setDirectDownloadId] = useState<number | null>(null);
  const [directDownloadError, setDirectDownloadError] = useState<string | null>(null);
  const [directDownloadDirectory, setDirectDownloadDirectory] = useState(() => localStorage.getItem(beatmapDownloadDirectoryKey) ?? "");
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const search = useOnlineBeatmapsets(activeQuery, true);
  const providers = useOnlineBeatmapProviderStatus();
  const settings = useSettings();
  const previewVolume = settings.data?.preview_volume ?? 65;

  useEffect(() => () => { audioRef.current?.pause(); audioRef.current = null; }, []);
  useEffect(() => { if (audioRef.current) audioRef.current.volume = previewVolume / 100; }, [previewVolume]);
  useEffect(() => {
    if (!linkedQuery) return;
    const frame = window.requestAnimationFrame(() => {
      const next = { ...createDefaultSearchQuery(ruleset), query: linkedQuery, title: linkedQuery };
      setDraft(next);
      setActiveQuery(next);
    });
    return () => window.cancelAnimationFrame(frame);
  }, [linkedQuery, ruleset]);

  const items = useMemo(() => uniqueBeatmapsets(search.data?.pages.flatMap((page) => page.beatmapsets ?? []) ?? []), [search.data]);
  const searchSuggestions = useMemo(() => items.flatMap((item) => [
    { value: item.title, detail: "标题" },
    ...(item.title_unicode ? [{ value: item.title_unicode, detail: "标题" }] : []),
    { value: item.artist, detail: "艺术家" },
    { value: item.creator, detail: "Mapper" },
    ...(item.tags?.split(" ").filter(Boolean).map((tag) => ({ value: tag, detail: "标签" })) ?? []),
  ]), [items]);
  const availableTotal = search.data?.pages[0]?.total ?? null;
  const onlineMirrorCount = providers.data?.filter((provider) => provider.id !== "official" && provider.online).length ?? 0;
  const queueItems = useMemo(() => [...queue.values()], [queue]);
  const detailFallback = items.find((item) => item.id === detailId) ?? queue.get(detailId ?? -1) ?? null;

  const togglePreview = (beatmapset: OnlineBeatmapset) => {
    const source = normalizePreviewUrl(beatmapset.preview_url);
    if (!source) return;
    if (playingId === beatmapset.id && audioRef.current) {
      audioRef.current.pause(); audioRef.current = null; setPlayingId(null); return;
    }
    audioRef.current?.pause();
    const audio = new Audio(source);
    audio.volume = previewVolume / 100; audio.onended = () => setPlayingId(null); audio.onerror = () => setPlayingId(null);
    audioRef.current = audio; setPlayingId(beatmapset.id); audio.play().catch(() => { audioRef.current = null; setPlayingId(null); });
  };

  const toggleQueue = (beatmapset: OnlineBeatmapset) => setQueue((current) => {
    const next = new Map(current);
    if (next.has(beatmapset.id)) next.delete(beatmapset.id); else next.set(beatmapset.id, beatmapset);
    return next;
  });

  const downloadBeatmapset = async (beatmapset: OnlineBeatmapset) => {
    if (directDownloadId !== null || beatmapset.availability?.download_disabled) return;
    setDirectDownloadId(beatmapset.id);
    setDirectDownloadError(null);
    try {
      let destination = directDownloadDirectory || settings.data?.beatmap_download_directory || "";
      if (!destination) {
        destination = await desktopApi.chooseBeatmapDownloadDirectory(null) ?? "";
        if (!destination) return;
        setDirectDownloadDirectory(destination);
        localStorage.setItem(beatmapDownloadDirectoryKey, destination);
        if (settings.data) {
          const saved = await desktopApi.updateSettings({ ...settings.data, beatmap_download_directory: destination });
          queryClient.setQueryData(settingsQueryKey, saved);
        }
      }
      const result = await desktopApi.downloadOnlineBeatmapsets({
        destination,
        provider: resolveDefaultDownloadProvider(settings.data),
        overwrite: false,
        include_video: settings.data?.include_video_in_beatmap_downloads ?? true,
        items: [{ beatmapset_id: beatmapset.id, artist: beatmapset.artist, title: beatmapset.title }],
      });
      if (result.failed) setDirectDownloadError(result.failures[0]?.message ?? "谱面下载失败");
    } catch (caught) {
      setDirectDownloadError((caught as CommandError).message ?? String(caught));
    } finally {
      setDirectDownloadId(null);
    }
  };

  const reset = () => { const next = createDefaultSearchQuery(ruleset); setDraft(next); setActiveQuery(next); };
  const changeSort = (sort: string) => {
    // 排序只作用于已经提交的查询，不能顺带提交仍在编辑的数值范围。
    setDraft((current) => ({ ...current, sort, cursor_string: null }));
    setActiveQuery((current) => ({ ...current, sort, cursor_string: null }));
  };
  const closeDetail = () => {
    setManualDetailId(null);
    const returnTo = location.state?.returnTo;
    if (typeof returnTo === "string" && returnTo.startsWith("/online/similar")) {
      navigate(returnTo);
      return;
    }
    if (searchParams.has("beatmapset") || searchParams.has("beatmap")) {
      const next = new URLSearchParams(searchParams);
      next.delete("beatmapset");
      next.delete("beatmap");
      setSearchParams(next, { replace: true });
    }
  };

  return <>
    <PageHeader
      actions={<div className="flex flex-wrap items-center justify-end gap-2"><Badge tone="cyan">osu! 官网数据</Badge><Badge tone={onlineMirrorCount ? "success" : "warning"}>{onlineMirrorCount} 个镜像可用</Badge><Badge tone="pink"><DownloadCloud className="size-3.5" />批量下载</Badge></div>}
      description="从官网获取完整谱面信息，选择谱面后使用可选镜像适配器下载。"
      eyebrow="Online beatmaps"
      title="在线谱面"
    />

    <div className="space-y-5">
      <OnlineBeatmapFilters loading={search.isFetching && !search.isFetchingNextPage} onChange={setDraft} onReset={reset} onSubmit={(next) => setActiveQuery({ ...next, cursor_string: null })} query={draft} suggestions={searchSuggestions} />
      <div className="grid grid-cols-1 items-start gap-5 2xl:grid-cols-[minmax(0,1fr)_clamp(16rem,15vw,18rem)]">
        <section className="min-w-0" data-page-guide-online-results="true">
          <OnlineBeatmapSortBar onChange={changeSort} sort={activeQuery.sort} />
          <div className="opp-online-panel mb-4 flex min-h-12 items-center justify-between rounded-[11px] border border-[var(--line-subtle)] bg-[color-mix(in_srgb,var(--surface-panel)_94%,transparent)] px-4 shadow-[0_14px_34px_rgba(0,0,0,0.08)]">
            <div className="text-sm text-slate-400">已加载 <strong className="font-mono text-slate-100">{items.length}</strong>{availableTotal !== null ? <> / 共 <strong className="font-mono text-slate-100">{availableTotal}</strong></> : null}</div>
            <Button disabled={!items.length} onClick={() => setQueue((current) => { const next = new Map(current); items.forEach((item) => { if (!item.availability?.download_disabled) next.set(item.id, item); }); return next; })} size="sm" variant="ghost"><CheckSquare2 className="size-4" />将当前结果全部加入队列</Button>
          </div>
          {directDownloadError ? <div className="mb-4 rounded-xl border border-amber-300/10 bg-amber-300/[0.05] px-4 py-3 text-sm text-amber-100">{directDownloadError}</div> : null}
          {search.isLoading ? <div className="grid grid-cols-1 gap-3.5 lg:grid-cols-2">{Array.from({ length: 6 }, (_, index) => <Skeleton className="aspect-[136/55] rounded-xl" key={index} />)}</div> : search.error ? <ErrorPanel error={search.error} onRetry={() => search.refetch()} /> : !items.length ? <EmptyState action={<Button onClick={reset}><Music2 className="size-4" />查看近期 Ranked</Button>} description="请放宽筛选条件，或更换内容筛选标签。" icon={<SearchX className="size-5" />} title="没有找到匹配的谱面" /> : <>
            {/* 常用桌面窗口保持双列，卡片尺寸随结果区宽度按比例缩放。 */}
            <div className="grid grid-cols-1 gap-3.5 lg:grid-cols-2">{items.map((beatmapset) => <BeatmapsetCard beatmapset={beatmapset} downloading={directDownloadId === beatmapset.id} key={beatmapset.id} onAddToCollection={() => openCollectionDialog(collectionCandidates(beatmapset))} onDownload={() => void downloadBeatmapset(beatmapset)} onOpen={() => setManualDetailId(beatmapset.id)} onPreview={() => togglePreview(beatmapset)} onSelect={() => toggleQueue(beatmapset)} playing={playingId === beatmapset.id} selected={queue.has(beatmapset.id)} />)}</div>
            {search.hasNextPage ? <Button className="mt-5 w-full" loading={search.isFetchingNextPage} onClick={() => search.fetchNextPage()}><ChevronDown className="size-4" />加载下一页</Button> : <p className="py-8 text-center text-sm text-slate-600">已到达搜索结果末尾</p>}
          </>}
        </section>
        <BeatmapDownloadPanel availableTotal={availableTotal} externalDownloadActive={directDownloadId !== null} onClear={() => setQueue(new Map())} onRemove={(id) => setQueue((current) => { const next = new Map(current); next.delete(id); return next; })} onReplace={(items) => setQueue(new Map(items.map((item) => [item.id, item])))} query={activeQuery} queue={queueItems} />
      </div>
    </div>

    <BeatmapsetDetailDialog beatmapsetId={detailId} fallback={detailFallback} initialBeatmapId={deepLink.beatmapId} key={detailId ?? "closed"} onAddToCollection={(beatmapset) => openCollectionDialog(collectionCandidates(beatmapset))} onClose={closeDetail} onFindSimilar={(beatmapId, beatmapRuleset) => navigate(similarityRouteForBeatmap(beatmapId, beatmapRuleset))} onPreview={togglePreview} playing={detailId !== null && playingId === detailId} />
  </>;
}

export function OnlineBeatmapsPage() {
  const { ruleset } = useMode();
  return <OnlineBeatmapsClient key={ruleset} ruleset={ruleset} />;
}
