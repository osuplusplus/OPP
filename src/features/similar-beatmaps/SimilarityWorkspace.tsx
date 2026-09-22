import { useEffect, useId, useMemo, useRef, useState, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { BeatmapSearchHeading } from "../../shared/components/BeatmapSearchHeading";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import {
  ArrowLeft,
  ArrowRight,
  ChevronLeft,
  ChevronRight,
  Download,
  FileUp,
  Heart,
  Headphones,
  History,
  ImageIcon,
  LoaderCircle,
  Layers3,
  AlertTriangle,
  Pause,
  Search,
  Trophy,
  X,
} from "lucide-react";

import { useMode } from "../../app/ModeContext";
import { Button } from "../../shared/components/ui";
import { StageBackground } from "../../shared/components/StageBackground";
import { errorMessage } from "../../shared/lib/format";
import { desktopApi } from "../../shared/lib/tauri";
import { useDebouncedValue } from "../../shared/lib/useDebouncedValue";
import type {
  AnySimilarityBeatmap,
  AnySimilarityResult,
  BeatmapQuery,
  LocalBeatmapSummary,
  LocalBeatmapSetSummary,
  OnlineBeatmapset,
  Ruleset,
  SimilaritySource,
} from "../../shared/types/osu";
import { BeatmapPreviewCard } from "../tools/ToolsPage";
import { useOnlineStageArtwork } from "../online-beatmaps/useOnlineStageArtwork";
import type { RecommendationHistoryEntry } from "./recommendationHistory";
import { SimilarityRadar } from "./SimilarityRadar";
import { MmaPatternPanel } from "./MmaPatternPanel";
import "./similarityWorkspace.css";

const explicitBeatmapId = (value: string) => {
  const input = value.trim();
  if (/^\d+$/.test(input)) return Number(input);
  try {
    const url = new URL(input);
    if (!/^(www\.)?osu\.ppy\.sh$/i.test(url.hostname)) return null;
    const fragment = url.hash.match(/\/(\d+)$/)?.[1];
    const path = url.pathname.match(/\/(?:beatmaps|b)\/(\d+)(?:\/|$)/)?.[1];
    return Number(fragment ?? path) || null;
  } catch {
    return null;
  }
};

const beatmapsetId = (value: string) => {
  try {
    const url = new URL(value.trim());
    if (!/^(www\.)?osu\.ppy\.sh$/i.test(url.hostname) || url.hash) return null;
    return Number(url.pathname.match(/\/beatmapsets\/(\d+)\/?$/)?.[1]) || null;
  } catch {
    return null;
  }
};

function localQuery(client: BeatmapQuery["client"], ruleset: Ruleset, search: string, offset: number): BeatmapQuery {
  return {
    client,
    rulesets: [ruleset],
    search,
    sort: "title",
    direction: "asc",
    offset,
    limit: 8,
    min_stars: null,
    max_stars: null,
    min_bpm: null,
    max_bpm: null,
    min_length_ms: null,
    max_length_ms: null,
    min_objects: null,
    max_objects: null,
    min_ar: null,
    max_ar: null,
    min_cs: null,
    max_cs: null,
    min_od: null,
    max_od: null,
    submitted: null,
  };
}

function durationLabel(seconds: number) {
  const value = Math.max(0, Math.round(seconds));
  return `${Math.floor(value / 60)}:${String(value % 60).padStart(2, "0")}`;
}

function LocalSetCover({ client, resourceId }: { client: BeatmapQuery["client"]; resourceId: string | null }) {
  const image = useQuery({
    queryKey: ["similarity-local-set-cover", client, resourceId],
    queryFn: () => desktopApi.getLocalBeatmapBackground(client, resourceId!, "thumbnail"),
    enabled: Boolean(resourceId),
    staleTime: Infinity,
    retry: false,
  });
  return <span className="similarity-search-cover">{image.data ? <img src={image.data} alt="" /> : <Search />}</span>;
}

export function SimilaritySearch({
  compact,
  busy,
  ruleset,
  value: controlledValue,
  onValueChange,
  onChoose,
  onChooseFile,
}: {
  compact: boolean;
  busy: boolean;
  ruleset: "osu" | "mania";
  value?: string;
  onValueChange?: (value: string) => void;
  onChoose: (source: SimilaritySource) => void;
  onChooseFile: () => void;
}) {
  const { client } = useMode();
  const [ownValue, setOwnValue] = useState("");
  const value = controlledValue ?? ownValue;
  const setValue = (next: string) => { setOwnValue(next); onValueChange?.(next); };
  const [open, setOpen] = useState(false);
  const [offset, setOffset] = useState(0);
  const [highlighted, setHighlighted] = useState(0);
  const [localSetChoices, setLocalSetChoices] = useState<LocalBeatmapSetSummary | null>(null);
  const [setChoices, setSetChoices] = useState<OnlineBeatmapset | null>(null);
  const [lookupError, setLookupError] = useState<string | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const root = useRef<HTMLDivElement>(null);
  const listId = useId();
  const debounced = useDebouncedValue(value, 250);
  const isExplicit = explicitBeatmapId(debounced) !== null || beatmapsetId(debounced) !== null;
  const query = useMemo(() => localQuery(client, ruleset, debounced.trim(), offset), [client, debounced, offset, ruleset]);
  const local = useQuery({
    queryKey: ["similarity-local-search", query],
    queryFn: () => desktopApi.queryLocalBeatmapSets(query),
    enabled: open && !isExplicit && !localSetChoices && !setChoices,
    placeholderData: (previous) => previous,
    retry: false,
  });
  const items = local.data?.items ?? [];
  const localDifficulties = localSetChoices?.difficulties.filter((item) => item.ruleset === ruleset) ?? [];
  const onlineDifficulties = (setChoices?.beatmaps ?? []).filter((item) => item.mode === ruleset).sort((a, b) => a.difficulty_rating - b.difficulty_rating);
  const stale = value !== debounced || local.isFetching;
  const choiceCount = localSetChoices ? localDifficulties.length : setChoices ? onlineDifficulties.length : items.length;
  const active = Math.min(highlighted, Math.max(0, choiceCount - 1));

  useEffect(() => {
    const close = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", close);
    return () => document.removeEventListener("pointerdown", close);
  }, []);

  const chooseLocal = async (item: LocalBeatmapSummary) => {
    setLookupError(null);
    try {
      const path = await desktopApi.getLocalBeatmapPath(item.resource.client, item.resource.resource_id);
      setOpen(false);
      onChoose({ kind: "local_file", path });
    } catch (error) {
      setLookupError(errorMessage(error));
    }
  };

  const chooseLocalSet = (item: LocalBeatmapSetSummary) => {
    setLocalSetChoices(item);
    setHighlighted(0);
    setLookupError(null);
  };

  const submit = async () => {
    const id = explicitBeatmapId(value);
    if (id) {
      setOpen(false);
      onChoose({ kind: "beatmap_id", value: String(id) });
      return;
    }
    const setId = beatmapsetId(value);
    if (setId) {
      setLookupError(null);
      try {
        const set = await desktopApi.getOnlineBeatmapset(setId);
        setSetChoices(set);
        setLocalSetChoices(null);
        setHighlighted(0);
        setOpen(true);
      } catch (error) {
        setLookupError(errorMessage(error));
      }
      return;
    }
    if (localSetChoices && localDifficulties[active]) await chooseLocal(localDifficulties[active]);
    else if (setChoices && onlineDifficulties[active]) {
      setOpen(false);
      onChoose({ kind: "beatmap_id", value: String(onlineDifficulties[active].id) });
    }
    else if (items[active] && !stale) chooseLocalSet(items[active]);
    else setOpen(true);
  };

  return <div className={`similarity-search ${compact ? "is-compact" : ""}`} ref={root}>
    <form className="beatmap-search-field" data-compact={compact} onSubmit={(event) => { event.preventDefault(); void submit(); }}>
      <Search aria-hidden="true" />
      <input
        ref={input}
        aria-label="搜索本地谱面、Beatmap ID 或链接"
        autoComplete="off"
        role="combobox"
        aria-expanded={open}
        aria-controls={open ? listId : undefined}
        aria-activedescendant={open && choiceCount ? `${listId}-${active}` : undefined}
        value={value}
        placeholder="搜索本地谱面，或粘贴 Beatmap ID / osu! 链接…"
        onFocus={() => setOpen(true)}
        onClick={() => setOpen(true)}
        onChange={(event) => { setValue(event.target.value); setOffset(0); setHighlighted(0); setLocalSetChoices(null); setSetChoices(null); setOpen(true); }}
        onKeyDown={(event) => {
          if (event.nativeEvent.isComposing) return;
          if (event.key === "ArrowDown" || event.key === "ArrowUp") {
            if (!choiceCount || (!localSetChoices && !setChoices && stale)) return;
            event.preventDefault();
            setHighlighted((active + (event.key === "ArrowDown" ? 1 : -1) + choiceCount) % choiceCount);
          }
          if (event.key === "Escape") setOpen(false);
        }}
      />
      {value ? <button className="similarity-search-clear" aria-label="清空搜索" type="button" onClick={() => { setValue(""); setOffset(0); setHighlighted(0); setLocalSetChoices(null); setSetChoices(null); setOpen(true); input.current?.focus(); }}><X /></button> : null}
      <Button type="button" variant="ghost" onClick={onChooseFile}><FileUp className="size-4" />选择 .osu</Button>
      <Button className="similarity-search-submit" type="submit" variant="ghost" disabled={!value.trim() || busy}>{busy ? <LoaderCircle className="size-4 animate-spin" /> : <Search className="size-4" />}查询相似</Button>
    </form>
    {open ? <div className="similarity-search-results" id={listId} role="listbox" aria-label="本地谱面搜索结果">
      {lookupError || local.error ? <p role="alert">{lookupError ?? errorMessage(local.error)}</p> : null}
      {localSetChoices ? <>
        <header><button type="button" className="similarity-search-back" onClick={() => { setLocalSetChoices(null); setHighlighted(0); }}><ChevronLeft />返回谱面集</button><small>{localSetChoices.artist} · {localSetChoices.title}</small></header>
        {localDifficulties.map((item, index) => <button id={`${listId}-${index}`} key={item.resource.resource_id} role="option" aria-selected={active === index} type="button" onPointerMove={() => setHighlighted(index)} onClick={() => void chooseLocal(item)}><strong>{item.difficulty_name}</strong><span>{item.stars?.toFixed(2) ?? "—"} ★ · BPM {Math.round(item.bpm)} · {durationLabel(item.length_ms / 1000)} · {item.beatmap_id ? `BID ${item.beatmap_id}` : "本地谱面"}</span></button>)}
      </> : setChoices ? <>
        <header><button type="button" className="similarity-search-back" onClick={() => { setSetChoices(null); setHighlighted(0); }}><ChevronLeft />返回搜索</button><small>{setChoices.artist} · {setChoices.title}</small></header>
        {onlineDifficulties.map((item, index) => <button id={`${listId}-${index}`} key={item.id} role="option" aria-selected={active === index} type="button" onPointerMove={() => setHighlighted(index)} onClick={() => { setOpen(false); onChoose({ kind: "beatmap_id", value: String(item.id) }); }}><strong>{item.version}</strong><span>BID {item.id} · {item.difficulty_rating.toFixed(2)} ★</span></button>)}
      </> : isExplicit ? <p>按 Enter 或点击“查询相似”解析该 ID / 链接。</p> : stale ? <p>正在搜索本地谱面集…</p> : items.length ? <>
        <header><span>{local.data?.total ?? 0} 个本地谱面集</span><small>{client === "stable" ? "Stable" : "lazer"} · {ruleset}</small></header>
        {items.map((item, index) => <button
          id={`${listId}-${index}`}
          key={item.set_key}
          role="option"
          aria-selected={active === index}
          type="button"
          onPointerMove={() => setHighlighted(index)}
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => chooseLocalSet(item)}
        ><LocalSetCover client={client} resourceId={item.background_resource_id} /><span className="similarity-search-copy"><strong>{item.title_unicode || item.title}</strong><span>{item.artist_unicode || item.artist} · {item.creators.join(" / ")}</span><small>{item.difficulties.length} 个难度 · {item.min_stars?.toFixed(2) ?? "—"}–{item.max_stars?.toFixed(2) ?? "—"} ★</small></span><ChevronRight className="similarity-search-next" /></button>)}
        {(local.data?.total ?? 0) > 8 ? <footer><button type="button" disabled={offset === 0} onClick={() => setOffset(Math.max(0, offset - 8))}>上一页</button><span>{Math.floor(offset / 8) + 1} / {Math.ceil((local.data?.total ?? 0) / 8)}</span><button type="button" disabled={offset + 8 >= (local.data?.total ?? 0)} onClick={() => setOffset(offset + 8)}>下一页</button></footer> : null}
      </> : <p>没有匹配的本地谱面集。也可以输入 Beatmap ID、官方链接或选择 .osu 文件。</p>}
    </div> : null}
  </div>;
}

export function SimilarityHome({
  busy,
  ruleset,
  onChoose,
  onChooseFile,
  onRecommend,
  onHistory,
  filterToday,
  onFilterTodayChange,
  searchValue,
  onSearchValueChange,
  status,
}: {
  busy: boolean;
  ruleset: "osu" | "mania";
  onChoose: (source: SimilaritySource) => void;
  onChooseFile: () => void;
  onRecommend: (kind: "recent" | "best") => void;
  onHistory: () => void;
  filterToday: boolean;
  onFilterTodayChange: (enabled: boolean) => void;
  searchValue?: string;
  onSearchValueChange?: (value: string) => void;
  status?: ReactNode;
}) {
  const reduced = useReducedMotion();
  return <motion.section className="similarity-home" initial={{ opacity: 0, y: reduced ? 0 : 12 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: reduced ? 0 : .28 }}>
    <BeatmapSearchHeading icon={<Layers3 />} title="OPP Similarity" description="搜索本地谱面，或粘贴 Beatmap ID / 官方链接，找到相似的下一首" />
    <SimilaritySearch compact={false} busy={busy} ruleset={ruleset} value={searchValue} onValueChange={onSearchValueChange} onChoose={onChoose} onChooseFile={onChooseFile} />
    <div className="similarity-recommendations">
      <button type="button" disabled={busy} onClick={() => onRecommend("recent")}><History /><span><strong>根据最近游玩推荐</strong><small>从最近通过的谱面寻找相似候选</small></span><ArrowRight /></button>
      <button type="button" disabled={busy} onClick={() => onRecommend("best")}><Trophy /><span><strong>根据你的 BP 推荐</strong><small>从最佳成绩偏好生成候选</small></span><ArrowRight /></button>
    </div>
    <RecommendationHistoryControls filterToday={filterToday} onFilterTodayChange={onFilterTodayChange} onHistory={onHistory} />
    {status ? <div className="similarity-index-status">{status}</div> : null}
  </motion.section>;
}

export function RecommendationHistoryControls({ filterToday, onFilterTodayChange, onHistory, compact = false }: {
  filterToday: boolean;
  onFilterTodayChange: (enabled: boolean) => void;
  onHistory: () => void;
  compact?: boolean;
}) {
  return <div className={`similarity-history-controls ${compact ? "is-compact" : ""}`}>
    <button className="similarity-history-link" type="button" onClick={onHistory}><History />{compact ? "历史" : "今日推荐历史"}</button>
    <label title="开启后，生成今日推荐时会跳过今天已经浏览过的谱面"><input checked={filterToday} onChange={(event) => onFilterTodayChange(event.target.checked)} type="checkbox" />过滤今日已推荐</label>
  </div>;
}

export function SimilarityHistoryDialog({
  open,
  title,
  entries,
  onClose,
  onChoose,
}: {
  open: boolean;
  title: string;
  entries: RecommendationHistoryEntry[];
  onClose: () => void;
  onChoose: (result: AnySimilarityResult) => void;
}) {
  if (!open) return null;
  return <div className="similarity-history-dialog" role="dialog" aria-modal="true" aria-label={title} onClick={onClose}>
    <motion.div initial={{ opacity: 0, y: 12, scale: .985 }} animate={{ opacity: 1, y: 0, scale: 1 }} transition={{ duration: .2 }} onClick={(event) => event.stopPropagation()}>
      <header><div><h2>{title}</h2><p>只记录今天真正浏览过的推荐谱面，共 {entries.length} 张。</p></div><button type="button" aria-label="关闭今日推荐历史" onClick={onClose}><X /></button></header>
      <main>{entries.length ? entries.map(({ displayed_at, key_count, result }) => <button type="button" key={`${result.ruleset}:${result.beatmap_id}:${displayed_at}`} disabled={result.ruleset === "mania" && !result.online_url} title={result.ruleset === "mania" && !result.online_url ? "本地谱面没有在线页面" : undefined} onClick={() => onChoose(result)}><span><strong>{result.artist} - {result.title}</strong><small>{key_count ? `${key_count}K · ` : ""}[{result.version}] · {result.creator}</small></span><time>{new Intl.DateTimeFormat("zh-CN", { hour: "2-digit", minute: "2-digit" }).format(new Date(displayed_at))}</time></button>) : <p>今天还没有浏览过推荐谱面。</p>}</main>
    </motion.div>
  </div>;
}

export function SimilarityMessage({ message, tone = "error", onClose }: { message: string | null; tone?: "error" | "status"; onClose: () => void }) {
  const reduced = useReducedMotion();
  const onCloseRef = useRef(onClose);
  useEffect(() => { onCloseRef.current = onClose; }, [onClose]);
  useEffect(() => {
    if (!message) return;
    const timer = window.setTimeout(() => onCloseRef.current(), tone === "status" ? 4_000 : 6_000);
    return () => window.clearTimeout(timer);
  }, [message, tone]);
  return <AnimatePresence>{message ? <motion.div
    className="similarity-message"
    data-tone={tone}
    role={tone === "error" ? "alert" : "status"}
    initial={{ opacity: 0, x: "-50%", y: reduced ? 0 : -12, scale: reduced ? 1 : .98 }}
    animate={{ opacity: 1, x: "-50%", y: 0, scale: 1 }}
    exit={{ opacity: 0, x: "-50%", y: reduced ? 0 : -8 }}
    transition={{ duration: reduced ? 0 : .2 }}
  ><AlertTriangle /><span>{message}</span><button type="button" aria-label="关闭消息" onClick={onClose}><X /></button></motion.div> : null}</AnimatePresence>;
}

function useBeatmapArtwork(beatmapsetId: number | null) {
  const set = useQuery({
    queryKey: ["similarity-workspace-set", beatmapsetId],
    queryFn: () => desktopApi.getOnlineBeatmapset(beatmapsetId!),
    enabled: beatmapsetId !== null && beatmapsetId > 0,
    staleTime: Infinity,
    retry: false,
  });
  return set.data;
}

function EvidenceRail({ source, result, cover, recommendation, details }: { source: AnySimilarityBeatmap; result: AnySimilarityResult; cover: string | null; recommendation: boolean; details: ReactNode }) {
  const mania = source.ruleset === "mania";
  const sourcePattern = mania ? source.pattern_view : null;
  const resultPattern = result.ruleset === "mania" ? result.pattern_view : null;
  return <motion.aside className="similarity-evidence-rail" layout initial={{ opacity: 0, x: -16 }} animate={{ opacity: 1, x: 0 }} transition={{ duration: .22 }}>
    <div className="similarity-source-cover">{cover ? <img src={cover} alt="" /> : <div />}</div>
    <div className="similarity-source-copy">
      <div className="similarity-source-kicker"><span>{recommendation ? "本首推荐依据" : "参考谱面"}</span><small>{source.online_url && source.beatmap_id > 0 ? `BID ${source.beatmap_id}` : "本地谱面"}</small></div>
      <h2>{source.title}</h2><p>{source.artist}</p>
      <dl>
        <div><dt>难度</dt><dd>{source.version}</dd></div>
        <div><dt>谱师</dt><dd>{source.creator}</dd></div>
        <div><dt>BPM</dt><dd>{Math.round(source.base.bpm)}</dd></div>
        <div><dt>{mania ? "有效长度" : "长度"}</dt><dd>{durationLabel(mania ? source.base.active_length_seconds : source.base.length_seconds)}</dd></div>
        {mania ? <><div><dt>键数 / Mod</dt><dd>{source.key_count}K · {source.game_mod}</dd></div><div><dt>难度分位</dt><dd>{Math.round(source.difficulty_percentile * 100)}%</dd></div></> : <><div><dt>星级</dt><dd>{source.star_rating?.toFixed(2) ?? "—"} ★</dd></div><div><dt>AR / OD</dt><dd>{source.base.ar.toFixed(1)} / {source.base.od.toFixed(1)}</dd></div></>}
      </dl>
    </div>
    {details ? <div className="similarity-evidence-context">{details}</div> : null}
    <div className="similarity-radar-overlay" aria-label="特征维度对比">
      <div className="similarity-radar-heading"><span>FEATURE PROFILE</span><strong>特征对比</strong></div>
      <div className="similarity-radar-chart"><SimilarityRadar compact target={source.difficulty} comparison={result.difficulty} patternView={sourcePattern} patternViewComparison={resultPattern} /></div>
    </div>
    {sourcePattern ? <section aria-label="参考谱面键型" className="similarity-evidence-context"><MmaPatternPanel compact view={sourcePattern} /></section> : null}
  </motion.aside>;
}

function CandidateMetrics({ result }: { result: AnySimilarityResult }) {
  const entries = result.ruleset === "mania"
    ? [["键数", `${result.key_count}K`], ["Mod", result.game_mod], ["BPM", Math.round(result.base.bpm)], ["有效长度", durationLabel(result.base.active_length_seconds)], ["平均 NPS", result.base.avg_nps.toFixed(2)], ["难度分位", `${Math.round(result.difficulty_percentile * 100)}%`]]
    : [["星级", `${result.star_rating?.toFixed(2) ?? "—"} ★`], ["BPM", Math.round(result.base.bpm)], ["长度", durationLabel(result.base.length_seconds)], ["AR", result.base.ar.toFixed(1)], ["OD", result.base.od.toFixed(1)], ["CS", result.base.cs.toFixed(1)]];
  return <div className="similarity-stage-metrics">{entries.map(([label, value]) => <div key={label}><span>{label}</span><strong>{value}</strong></div>)}</div>;
}

export function SimilarityStage({
  result,
  source,
  index,
  total,
  recommendation,
  playing,
  previewLoading,
  downloading,
  completing,
  toolbar,
  details,
  emptyMessage,
  onPrevious,
  onNext,
  onPreview,
  onDownload,
  onCollect,
  onHome,
  onDisplayed,
}: {
  result: AnySimilarityResult;
  source: AnySimilarityBeatmap;
  index: number;
  total: number;
  recommendation: boolean;
  playing: boolean;
  previewLoading: boolean;
  downloading: boolean;
  completing: boolean;
  toolbar: ReactNode;
  details: ReactNode;
  emptyMessage?: string | null;
  onPrevious: () => void;
  onNext: () => void;
  onPreview: () => void;
  onDownload: () => void;
  onCollect: () => void;
  onHome: () => void;
  onDisplayed?: () => void;
}) {
  const reduced = useReducedMotion();
  const [visualPreview, setVisualPreview] = useState(false);
  const online = result.ruleset !== "mania" || Boolean(result.online_url);
  const reference = useBeatmapArtwork(source.ruleset !== "mania" || source.online_url ? source.beatmapset_id : null);
  const referenceCover = reference?.covers?.["card@2x"] ?? reference?.covers?.card ?? reference?.covers?.["cover@2x"] ?? reference?.covers?.cover ?? null;
  const candidate = useBeatmapArtwork(online && result.beatmapset_id > 0 ? result.beatmapset_id : null);
  const candidateArtwork = useOnlineStageArtwork(candidate, online);
  const resultAnimationKey = `${result.ruleset}:${result.beatmap_id}:${result.ruleset === "mania" ? result.game_mod : "NM"}`;
  const onDisplayedRef = useRef(onDisplayed);
  useEffect(() => { onDisplayedRef.current = onDisplayed; }, [onDisplayed]);

  useEffect(() => {
    const timer = window.setTimeout(() => onDisplayedRef.current?.(), reduced ? 0 : 230);
    return () => window.clearTimeout(timer);
  }, [reduced, resultAnimationKey]);

  useEffect(() => {
    const handle = (event: KeyboardEvent) => {
      if (emptyMessage) return;
      if (event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey || document.querySelector('[role="dialog"]')) return;
      const target = event.target as HTMLElement | null;
      if (target?.closest("input, textarea, select, button, [contenteditable=true]")) return;
      if (event.key === "ArrowLeft" && index > 0) { event.preventDefault(); onPrevious(); }
      if (event.key === "ArrowRight" && index + 1 < total) { event.preventDefault(); onNext(); }
    };
    window.addEventListener("keydown", handle);
    return () => window.removeEventListener("keydown", handle);
  }, [emptyMessage, index, onNext, onPrevious, total]);

  return <section className="similarity-workspace" style={candidateArtwork.style}>
    <StageBackground source={candidateArtwork.source} />
    <header className="similarity-workspace-toolbar"><button className="similarity-home-button" type="button" onClick={onHome} aria-label="返回搜索首页" title="返回搜索首页"><ArrowLeft /></button>{toolbar}</header>
    <div className="similarity-workspace-grid">
      <EvidenceRail key={`${source.ruleset}:${source.beatmap_id}:${source.version}`} source={source} result={result} cover={referenceCover} recommendation={recommendation} details={details} />
      <main className="similarity-candidate-stage">
        <AnimatePresence mode="popLayout" initial={false}>
          {emptyMessage ? <motion.div key="empty" className="similarity-stage-empty" initial={{ opacity: 0, y: reduced ? 0 : 10 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0 }} transition={{ duration: reduced ? 0 : .2 }}><span>0 / {total}</span><h2>没有符合条件的候选谱面</h2><p>{emptyMessage}</p></motion.div> : <motion.div key={resultAnimationKey} className="similarity-candidate-copy" initial={{ opacity: 0, x: reduced ? 0 : 22 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: reduced ? 0 : -18 }} transition={{ duration: reduced ? 0 : .22 }}>
            <div className="similarity-candidate-meta"><span>{online ? `BID ${result.beatmap_id}` : "本地谱面"}</span><span>距离 {result.final_distance.toFixed(4)}</span>{completing ? <span><LoaderCircle className="animate-spin" />正在完善推荐</span> : null}</div>
            <p>{result.artist}</p>
            <h1>{result.title}</h1>
            <div className="similarity-candidate-difficulty">
              <strong>[{result.version}]</strong>
              <span>mapped by {result.creator}</span>
            </div>
            <CandidateMetrics result={result} />
            {result.ruleset === "mania" && result.pattern_view ? <section aria-label="候选谱面键型" className="similarity-candidate-patterns"><MmaPatternPanel compact view={result.pattern_view} /></section> : null}
            <div className="similarity-stage-actions">
              <button type="button" disabled={!online || previewLoading} onClick={onPreview}>{playing ? <Pause /> : <Headphones />}{playing ? "暂停试听" : "试听"}</button>
              <button className="is-primary" type="button" disabled={!online || downloading} onClick={onDownload}><Download />{downloading ? "下载中" : "下载"}</button>
              <button type="button" disabled={!online} onClick={() => setVisualPreview(true)}><ImageIcon />预览</button>
              <button type="button" disabled={!online} onClick={onCollect}><Heart />收藏</button>
              <div className="similarity-stage-navigation" aria-label="候选谱面切换">
                <button type="button" disabled={index === 0} onClick={onPrevious}><ArrowLeft />上一首</button>
                <span>{index + 1} / {total}</span>
                <button type="button" disabled={index + 1 >= total} onClick={onNext}>下一首<ArrowRight /></button>
              </div>
            </div>
          </motion.div>
          }
        </AnimatePresence>
      </main>
    </div>
    {visualPreview && online ? <div className="similarity-visual-preview" role="dialog" aria-label="谱面预览" onClick={() => setVisualPreview(false)}><div onClick={(event) => event.stopPropagation()}><button className="similarity-preview-close" aria-label="关闭谱面预览" type="button" onClick={() => setVisualPreview(false)}><X /></button><BeatmapPreviewCard embeddedBid={result.beatmap_id} /></div></div> : null}
  </section>;
}
