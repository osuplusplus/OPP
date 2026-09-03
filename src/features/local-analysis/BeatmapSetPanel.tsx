import {
  useDeferredValue,
  useMemo,
  useState,
} from "react";
import {
  ChevronLeft,
  ChevronRight,
  CircleDot,
  ExternalLink,
  FolderSearch,
  PackageOpen,
  Gauge,
  Heart,
  ImageIcon,
  Hash,
  ListFilter,
  ListMusic,
  Music4,
  RotateCcw,
  ScanSearch,
  SlidersHorizontal,
  Sparkles,
  Timer,
  Trophy,
  WandSparkles,
  X,
  Zap,
} from "lucide-react";
import * as Dialog from "@radix-ui/react-dialog";
import { ErrorPanel } from "../../shared/components/ErrorPanel";
import {
  Badge,
  Button,
  Card,
  EmptyState,
  Skeleton,
} from "../../shared/components/ui";
import { SearchAutocomplete } from "../../shared/components/SearchAutocomplete";
import { BeatmapDifficultyStrip, BeatmapInfoBar } from "../../shared/components/BeatmapSetVisuals";
import { errorMessage, fullNumber, rulesetLabels } from "../../shared/lib/format";
import { desktopApi } from "../../shared/lib/tauri";
import { DifficultyIcon, ModeIcon } from "../online-beatmaps/BeatmapVisuals";
import { similarityRouteForLocalResource } from "../similar-beatmaps/navigation";
import { openCollectionDialog } from "../collections/events";
import { beatmapPreviewRoute } from "../tools/navigation";
import type {
  BeatmapQuery,
  BeatmapSort,
  LocalBeatmapSetSummary,
  OsuClient,
  Ruleset,
} from "../../shared/types/osu";
import {
  useLocalBeatmapBackground,
  useLocalBeatmapSets,
} from "./api";

interface RangeValues {
  minStars: string;
  maxStars: string;
  minBpm: string;
  maxBpm: string;
  minLength: string;
  maxLength: string;
  minObjects: string;
  maxObjects: string;
  minAr: string;
  maxAr: string;
  minCs: string;
  maxCs: string;
  minOd: string;
  maxOd: string;
}

const emptyRanges: RangeValues = {
  minStars: "",
  maxStars: "",
  minBpm: "",
  maxBpm: "",
  minLength: "",
  maxLength: "",
  minObjects: "",
  maxObjects: "",
  minAr: "",
  maxAr: "",
  minCs: "",
  maxCs: "",
  minOd: "",
  maxOd: "",
};

const starPresets = [
  ["全部", "", ""],
  ["入门", "", "2.69"],
  ["进阶", "2.7", "3.99"],
  ["高难", "4", "5.29"],
  ["专家", "5.3", "6.49"],
  ["极限", "6.5", ""],
] as const;

function numberValue(value: string) {
  const parsed = Number(value);
  return value.trim() && Number.isFinite(parsed) ? parsed : null;
}

function formatDuration(milliseconds: number) {
  const seconds = Math.max(0, Math.round(milliseconds / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}

function Metric({
  icon: Icon,
  label,
  value,
}: {
  icon: typeof Gauge;
  label: string;
  value: string;
}) {
  return (
    <div className="flex items-center gap-2">
      <Icon className="size-3.5 text-slate-500" />
      <div>
        <p className="text-[9px] uppercase tracking-wider text-slate-600">{label}</p>
        <p className="mt-0.5 font-mono text-xs font-semibold text-slate-200">{value}</p>
      </div>
    </div>
  );
}

function SetBackground({
  client,
  resourceId,
}: {
  client: OsuClient;
  resourceId: string | null;
}) {
  const query = useLocalBeatmapBackground(client, resourceId);
  return query.data ? (
    <img
      alt=""
      className="theme-beatmap-background absolute inset-0 size-full object-cover opacity-55"
      src={query.data}
    />
  ) : (
    <div className="theme-beatmap-background absolute inset-0 bg-[radial-gradient(circle_at_80%_20%,rgba(92,225,230,.16),transparent_32%),radial-gradient(circle_at_15%_80%,rgba(255,106,167,.18),transparent_36%)]" />
  );
}

function LocalDifficultyDialog({
  client,
  set,
  open,
  onClose,
  onOpen,
  onFindSimilar,
}: {
  client: OsuClient;
  set: LocalBeatmapSetSummary;
  open: boolean;
  onClose: () => void;
  onOpen: (resourceId: string) => void;
  onFindSimilar: (resourceId: string, ruleset: Ruleset) => void;
}) {
  const buttonClass = "w-full justify-center whitespace-nowrap px-2";
  return (
    <Dialog.Root onOpenChange={(next) => !next && onClose()} open={open}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-[120] bg-black/65 backdrop-blur-sm" />
        <Dialog.Content className="fixed left-1/2 top-1/2 z-[130] flex max-h-[min(800px,calc(100vh-32px))] w-[min(1080px,calc(100vw-32px))] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-3xl border border-white/10 bg-[#0d131f] shadow-2xl outline-none">
          <div className="flex items-start justify-between gap-5 border-b border-white/[0.08] p-6">
            <div className="min-w-0"><Dialog.Title className="truncate text-xl font-semibold text-white">{set.title_unicode || set.title}</Dialog.Title><Dialog.Description className="mt-1 truncate text-sm text-slate-400">{set.artist_unicode || set.artist} · {set.difficulties.length} 个难度</Dialog.Description></div>
            <Dialog.Close aria-label="关闭难度列表" className="grid size-9 shrink-0 place-items-center rounded-xl border border-white/10 text-slate-400 transition hover:bg-white/[0.08] hover:text-white"><X className="size-4" /></Dialog.Close>
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto p-4 sm:p-6"><div className="space-y-3">
            {set.difficulties.map((difficulty) => <article className="rounded-2xl border border-white/[0.07] bg-white/[0.025] p-4 transition hover:border-cyan-300/25 hover:bg-cyan-300/[0.04]" key={difficulty.resource.resource_id}>
              <div className="flex flex-wrap items-start gap-4">
                <button className="min-w-0 flex-1 text-left" onClick={() => onOpen(difficulty.resource.resource_id)} type="button"><div className="flex items-center gap-2"><ModeIcon mode={difficulty.ruleset} /><DifficultyIcon mode={difficulty.ruleset} stars={difficulty.stars} /><h3 className="truncate text-base font-semibold text-white">[{difficulty.difficulty_name}]</h3></div><p className="mt-2 text-xs text-slate-500">{rulesetLabels[difficulty.ruleset]} · {difficulty.creator}</p></button>
                <div className="grid flex-1 grid-cols-3 gap-x-5 gap-y-3 sm:grid-cols-6"><Metric icon={Gauge} label="AR / OD" value={`${difficulty.ar.toFixed(1)} / ${difficulty.od.toFixed(1)}`} /><Metric icon={CircleDot} label="CS" value={difficulty.cs.toFixed(1)} /><Metric icon={Zap} label="BPM" value={difficulty.bpm.toFixed(0)} /><Metric icon={Trophy} label="Max PP" value={difficulty.max_pp === null ? "—" : difficulty.max_pp.toFixed(1)} /><Metric icon={Timer} label="NPS" value={difficulty.average_nps.toFixed(1)} /><Metric icon={Hash} label="Objects" value={fullNumber(difficulty.object_count)} /></div>
              </div>
              <div className="mt-4 grid grid-cols-2 gap-2 border-t border-white/[0.06] pt-3 sm:grid-cols-3 xl:grid-cols-5">
                <Button className={buttonClass} onClick={() => onOpen(difficulty.resource.resource_id)} size="sm" variant="secondary">查看详情</Button>
                <Button className={buttonClass} onClick={() => onFindSimilar(difficulty.resource.resource_id, difficulty.ruleset)} size="sm" variant="primary"><ScanSearch className="size-3.5" />查找相似</Button>
                <Button className={buttonClass} onClick={() => openCollectionDialog([{ beatmap_id: difficulty.beatmap_id, beatmapset_id: difficulty.beatmap_set_id, checksum: null, ruleset: difficulty.ruleset, difficulty_name: difficulty.difficulty_name, title: difficulty.title_unicode || difficulty.title, artist: difficulty.artist_unicode || difficulty.artist, creator: difficulty.creator, local_client: client, local_resource_id: difficulty.resource.resource_id }])} size="sm" variant="secondary"><Heart className="size-3.5" />加入收藏夹</Button>
                <Button className={buttonClass} onClick={() => { const params = new URLSearchParams({ client, resource: difficulty.resource.resource_id }); window.location.hash = `/view-trainer?${params}`; }} size="sm" variant="primary"><WandSparkles className="size-3.5" />导入 Trainer</Button>
                <Button className={buttonClass} disabled={!beatmapPreviewRoute(difficulty.beatmap_id)} onClick={() => { const route = beatmapPreviewRoute(difficulty.beatmap_id); if (route) window.location.hash = route; }} size="sm" title={difficulty.beatmap_id ? "前往工具集合生成铺面预览" : "该本地谱面没有 Beatmap ID，暂不支持生成预览"} variant="secondary"><ImageIcon className="size-3.5" />生成预览</Button>
              </div>
            </article>)}
          </div></div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

function BeatmapSetCard({
  client,
  set,
  onOpen,
  onFindSimilar,
}: {
  client: OsuClient;
  set: LocalBeatmapSetSummary;
  onOpen: (resourceId: string) => void;
  onFindSimilar: (resourceId: string, ruleset: Ruleset) => void;
}) {
  const [difficultyDialogOpen, setDifficultyDialogOpen] = useState(false);
  const [neteaseError, setNeteaseError] = useState<unknown>(null);
  const [exporting, setExporting] = useState(false);
  const [exportNotice, setExportNotice] = useState<string | null>(null);
  const exportOsz = async () => {
    const outDir = await desktopApi.chooseDirectory("选择 .osz 导出位置");
    if (!outDir) return;
    setExporting(true);
    setExportNotice(null);
    try {
      const path = await desktopApi.exportLocalBeatmapSet(client, set.set_key, outDir);
      setExportNotice(`已导出：${path}`);
    } catch (caught) {
      setExportNotice(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setExporting(false);
    }
  };
  const starRange =
    set.min_stars === null
      ? "待计算"
      : set.min_stars === set.max_stars
        ? `${set.min_stars.toFixed(2)}★`
        : `${set.min_stars.toFixed(2)}–${set.max_stars?.toFixed(2)}★`;
  const peakNps = Math.max(...set.difficulties.map((item) => item.peak_nps), 0);

  return (
    <>
    <article className="overflow-hidden rounded-[22px] border border-white/[0.075] bg-[#0f1522] shadow-[0_16px_45px_rgba(0,0,0,.16)] transition hover:border-white/[0.14]">
      <div className="relative min-h-40 overflow-hidden">
        <SetBackground client={client} resourceId={set.background_resource_id} />
        <div className="theme-beatmap-overlay-primary absolute inset-0 bg-gradient-to-r from-[#0b101b]/95 via-[#0b101b]/76 to-[#0b101b]/35" />
        <div className="theme-beatmap-overlay-secondary absolute inset-0 bg-gradient-to-t from-[#0f1522] via-transparent to-black/20" />
        <div className="relative flex min-h-40 items-end gap-6 p-5">
          <div className="min-w-0 flex-1">
            <div className="mb-3 flex flex-wrap items-center gap-2">
              <Badge tone={set.beatmap_set_id ? "success" : "neutral"}>
                {set.beatmap_set_id ? `SET ${set.beatmap_set_id}` : "LOCAL SET"}
              </Badge>
              {set.grouping_inferred ? <Badge tone="warning">推断分组</Badge> : null}
              <Badge tone="cyan">{set.difficulties.length} 个匹配难度</Badge>
            </div>
            <h3 className="truncate text-xl font-semibold tracking-tight text-white">
              {set.title_unicode || set.title}
            </h3>
            <p className="mt-1.5 truncate text-sm text-slate-300">
              {set.artist_unicode || set.artist}
            </p>
            <p className="mt-2 truncate text-xs text-slate-500">
              mapped by <span className="text-slate-300">{set.creators.join(" · ")}</span>
            </p>
          </div>
          <BeatmapInfoBar metrics={[{ label: "Star range", value: starRange }, { label: "BPM", value: set.bpm.toFixed(0) }, { label: "Length", value: formatDuration(set.length_ms) }, { label: "Objects", value: fullNumber(set.object_count) }, { label: "Peak NPS", value: peakNps.toFixed(1) }, { label: "Difficulties", value: String(set.difficulties.length) }]} />
        </div>
      </div>

      <div className="flex items-center gap-2 border-t border-white/[0.055] bg-black/10 px-5 py-3">
        <div className="min-w-0 flex-1">
          <BeatmapDifficultyStrip difficulties={set.difficulties.map((difficulty) => ({ id: difficulty.resource.resource_id, mode: difficulty.ruleset, stars: difficulty.stars, label: difficulty.difficulty_name, onClick: () => onOpen(difficulty.resource.resource_id) }))} />
        </div>
        {client === "stable" ? (
          <Button
            aria-label="在资源管理器中打开谱面"
            disabled={!set.difficulties[0]?.resource.logical_path}
            onClick={() => { const path = set.difficulties[0]?.resource.logical_path; if (path) void desktopApi.openLocalResourceInExplorer(client, path); }}
            size="sm"
          >
            <FolderSearch className="size-3.5" />
            打开文件
          </Button>
        ) : (
          // Lazer 文件按哈希散落存储，没有可打开的谱面集目录，直接提供导出。
          <Button
            aria-label="导出为 .osz 谱面包"
            disabled={exporting}
            loading={exporting}
            onClick={() => void exportOsz()}
            size="sm"
            variant="primary"
          >
            <PackageOpen className="size-3.5" />
            导出 .osz
          </Button>
        )}
        {exportNotice ? <span className="max-w-56 truncate text-[11px] text-slate-500" title={exportNotice}>{exportNotice}</span> : null}
        {set.beatmap_set_id ? (
          <Button
            aria-label="在 osu! 官网打开谱面"
            onClick={() => void desktopApi.openExternal(
              `https://osu.ppy.sh/beatmapsets/${set.beatmap_set_id}`,
            )}
            size="sm"
            variant="secondary"
          >
            <ExternalLink className="size-3.5" />
            打开官网
          </Button>
        ) : null}
        <Button
          aria-label="在浏览器中搜索网易云音乐"
          onClick={() => {
            setNeteaseError(null);
            void desktopApi
              .openNeteaseMusicSearch(set.artist_unicode || set.artist, set.title_unicode || set.title)
              .catch(setNeteaseError);
          }}
          size="sm"
          variant="secondary"
        >
          <Music4 className="size-3.5" />
          搜索网易云音乐
        </Button>
        <Button
          aria-haspopup="dialog"
          onClick={() => setDifficultyDialogOpen(true)}
          className="ml-auto shrink-0"
          size="sm"
        >
          <ListMusic className="size-3.5" />
          查看难度
        </Button>
      </div>
      {neteaseError ? <p className="border-t border-rose-300/15 bg-rose-300/[0.05] px-5 py-2 text-xs text-rose-100" role="alert">{errorMessage(neteaseError)}</p> : null}
    </article>
    <LocalDifficultyDialog client={client} onClose={() => setDifficultyDialogOpen(false)} onFindSimilar={onFindSimilar} onOpen={(resourceId) => { setDifficultyDialogOpen(false); onOpen(resourceId); }} open={difficultyDialogOpen} set={set} />
    </>
  );
}

function RangeInput({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
}) {
  return (
    <label className="block">
      <span className="mb-1.5 block text-[10px] uppercase tracking-wider text-slate-600">
        {label}
      </span>
      <input
        className="w-full rounded-xl border border-white/[0.075] bg-black/20 px-3 py-2.5 font-mono text-xs text-white outline-none placeholder:text-slate-700 focus:border-cyan-300/25"
        min="0"
        onChange={(event) => onChange(event.target.value)}
        placeholder="不限"
        step="0.1"
        type="number"
        value={value}
      />
    </label>
  );
}

export function BeatmapSetPanel({
  client,
  ruleset,
  onOpen,
}: {
  client: OsuClient;
  ruleset: Ruleset;
  onOpen: (resourceId: string) => void;
}) {
  const [search, setSearch] = useState("");
  const deferredSearch = useDeferredValue(search);
  const [ranges, setRanges] = useState<RangeValues>(emptyRanges);
  const [submitted, setSubmitted] = useState<"all" | "online" | "local">("all");
  const [sort, setSort] = useState<BeatmapSort>("title");
  const [direction, setDirection] = useState<"asc" | "desc">("asc");
  const [offset, setOffset] = useState(0);
  const [limit, setLimit] = useState(40);
  const [advanced, setAdvanced] = useState(false);

  const updateRange = (key: keyof RangeValues, value: string) => {
    setRanges((current) => ({ ...current, [key]: value }));
    setOffset(0);
  };
  const query = useMemo<BeatmapQuery>(
    () => ({
      client,
      search: deferredSearch,
      rulesets: [ruleset],
      min_stars: numberValue(ranges.minStars),
      max_stars: numberValue(ranges.maxStars),
      min_bpm: numberValue(ranges.minBpm),
      max_bpm: numberValue(ranges.maxBpm),
      min_length_ms:
        numberValue(ranges.minLength) === null
          ? null
          : numberValue(ranges.minLength)! * 1000,
      max_length_ms:
        numberValue(ranges.maxLength) === null
          ? null
          : numberValue(ranges.maxLength)! * 1000,
      min_objects: numberValue(ranges.minObjects),
      max_objects: numberValue(ranges.maxObjects),
      min_ar: numberValue(ranges.minAr),
      max_ar: numberValue(ranges.maxAr),
      min_cs: numberValue(ranges.minCs),
      max_cs: numberValue(ranges.maxCs),
      min_od: numberValue(ranges.minOd),
      max_od: numberValue(ranges.maxOd),
      submitted: submitted === "all" ? null : submitted === "online",
      sort,
      direction,
      offset,
      limit,
    }),
    [
      client,
      deferredSearch,
      direction,
      limit,
      offset,
      ranges,
      ruleset,
      sort,
      submitted,
    ],
  );
  const sets = useLocalBeatmapSets(query, true);
  const searchSuggestions = (sets.data?.items ?? []).flatMap((item) => [
    { value: item.title, detail: "标题" },
    { value: item.title_unicode, detail: "标题" },
    { value: item.artist, detail: "艺术家" },
    { value: item.artist_unicode, detail: "艺术家" },
    ...item.creators.map((creator) => ({ value: creator, detail: "Mapper" })),
  ]);
  const activeRangeCount = Object.values(ranges).filter(Boolean).length;
  const activeFilterCount = activeRangeCount + (submitted === "all" ? 0 : 1);

  const reset = () => {
    setRanges(emptyRanges);
    setSubmitted("all");
    setSearch("");
    setOffset(0);
  };

  return (
    <div>
      <Card className="mb-4 overflow-hidden">
        <div className="flex items-center gap-3 p-3">
          <div className="min-w-64 flex-1">
            <SearchAutocomplete
              aria-label="搜索谱面集"
              className="w-full rounded-xl border border-white/[0.07] bg-black/20 py-2.5 pl-10 pr-4 text-sm text-white outline-none placeholder:text-slate-700 focus:border-cyan-300/25"
              onChange={(value) => {
                setSearch(value);
                setOffset(0);
              }}
              placeholder="搜索标题、艺术家、mapper、难度、来源、标签或 ID"
              suggestions={searchSuggestions}
              value={search}
            />
          </div>
          <Badge tone="cyan">{rulesetLabels[ruleset]}</Badge>
          <select
            aria-label="谱面集来源"
            className="rounded-xl border border-white/[0.07] bg-[#111725] px-3 py-2.5 text-xs text-slate-300 outline-none"
            onChange={(event) => {
              setSubmitted(event.target.value as typeof submitted);
              setOffset(0);
            }}
            value={submitted}
          >
            <option value="all">全部来源</option>
            <option value="online">已提交</option>
            <option value="local">本地 / 未提交</option>
          </select>
          <select
            aria-label="谱面排序"
            className="rounded-xl border border-white/[0.07] bg-[#111725] px-3 py-2.5 text-xs text-slate-300 outline-none"
            onChange={(event) => {
              setSort(event.target.value as BeatmapSort);
              setOffset(0);
            }}
            value={sort}
          >
            <option value="title">标题</option>
            <option value="artist">艺术家</option>
            <option value="creator">Mapper</option>
            <option value="stars">最高星数</option>
            <option value="bpm">BPM</option>
            <option value="length">时长</option>
            <option value="object_count">物件数</option>
            <option value="modified_at">修改时间</option>
          </select>
          <select
            aria-label="排序方向"
            className="rounded-xl border border-white/[0.07] bg-[#111725] px-3 py-2.5 text-xs text-slate-300 outline-none"
            onChange={(event) => {
              setDirection(event.target.value as "asc" | "desc");
              setOffset(0);
            }}
            value={direction}
          >
            <option value="asc">升序</option>
            <option value="desc">降序</option>
          </select>
          <Button
            onClick={() => setAdvanced((value) => !value)}
            size="sm"
            variant={advanced ? "primary" : "secondary"}
          >
            <SlidersHorizontal className="size-3.5" />
            筛选 {activeFilterCount ? `· ${activeFilterCount}` : ""}
          </Button>
        </div>

        <div className="flex items-center gap-2 border-t border-white/[0.055] px-3 py-2.5">
          <span className="mr-1 inline-flex items-center gap-1.5 text-[10px] uppercase tracking-wider text-slate-600">
            <Sparkles className="size-3.5" />
            星级
          </span>
          {starPresets.map(([label, min, max]) => {
            const active = ranges.minStars === min && ranges.maxStars === max;
            return (
              <button
                className={`rounded-lg border px-2.5 py-1.5 text-[10px] font-semibold transition ${
                  active
                    ? "border-cyan-300/25 bg-cyan-300/10 text-cyan-100"
                    : "border-white/[0.06] bg-white/[0.025] text-slate-500 hover:text-slate-300"
                }`}
                key={label}
                onClick={() => {
                  setRanges((current) => ({
                    ...current,
                    minStars: min,
                    maxStars: max,
                  }));
                  setOffset(0);
                }}
                type="button"
              >
                {label}
              </button>
            );
          })}
          <span className="ml-auto text-xs text-slate-600">
            {sets.data ? `${fullNumber(sets.data.total)} 个谱面集` : "正在统计"}
          </span>
        </div>

        {advanced ? (
          <div className="border-t border-white/[0.055] bg-black/10 p-4">
            <div className="grid grid-cols-7 gap-3">
              <RangeInput label="最低星数" value={ranges.minStars} onChange={(value) => updateRange("minStars", value)} />
              <RangeInput label="最高星数" value={ranges.maxStars} onChange={(value) => updateRange("maxStars", value)} />
              <RangeInput label="最低 BPM" value={ranges.minBpm} onChange={(value) => updateRange("minBpm", value)} />
              <RangeInput label="最高 BPM" value={ranges.maxBpm} onChange={(value) => updateRange("maxBpm", value)} />
              <RangeInput label="最短秒数" value={ranges.minLength} onChange={(value) => updateRange("minLength", value)} />
              <RangeInput label="最长秒数" value={ranges.maxLength} onChange={(value) => updateRange("maxLength", value)} />
              <RangeInput label="最少物件" value={ranges.minObjects} onChange={(value) => updateRange("minObjects", value)} />
              <RangeInput label="最多物件" value={ranges.maxObjects} onChange={(value) => updateRange("maxObjects", value)} />
              <RangeInput label="最低 AR" value={ranges.minAr} onChange={(value) => updateRange("minAr", value)} />
              <RangeInput label="最高 AR" value={ranges.maxAr} onChange={(value) => updateRange("maxAr", value)} />
              <RangeInput label="最低 CS" value={ranges.minCs} onChange={(value) => updateRange("minCs", value)} />
              <RangeInput label="最高 CS" value={ranges.maxCs} onChange={(value) => updateRange("maxCs", value)} />
              <RangeInput label="最低 OD" value={ranges.minOd} onChange={(value) => updateRange("minOd", value)} />
              <RangeInput label="最高 OD" value={ranges.maxOd} onChange={(value) => updateRange("maxOd", value)} />
            </div>
            <div className="mt-3 flex items-center justify-between">
              <p className="inline-flex items-center gap-2 text-[11px] text-slate-600">
                <ListFilter className="size-3.5" />
                条件按难度匹配，结果再按 BeatmapSet ID 或本地目录合并。
              </p>
              <Button disabled={!activeFilterCount && !search} onClick={reset} size="sm">
                <RotateCcw className="size-3.5" />
                清空筛选
              </Button>
            </div>
          </div>
        ) : null}
      </Card>

      {sets.isLoading ? (
        <div className="space-y-3">
          {Array.from({ length: 5 }, (_, index) => (
            <Skeleton className="h-52 rounded-[22px]" key={index} />
          ))}
        </div>
      ) : sets.error ? (
        <ErrorPanel error={sets.error} onRetry={() => sets.refetch()} />
      ) : sets.data?.items.length ? (
        <>
          <div className="space-y-3">
            {sets.data.items.map((set) => (
              <BeatmapSetCard
                client={client}
                key={set.set_key}
                onFindSimilar={(resourceId, difficultyRuleset) => {
                  window.location.hash = similarityRouteForLocalResource(client, resourceId, difficultyRuleset);
                }}
                onOpen={onOpen}
                set={set}
              />
            ))}
          </div>
          <div className="mt-4 flex items-center justify-between">
            <div className="flex items-center gap-3">
              <p className="text-xs text-slate-600">
                第 {Math.floor(sets.data.offset / sets.data.limit) + 1} /{" "}
                {Math.max(1, Math.ceil(sets.data.total / sets.data.limit))} 页
              </p>
              <select
                aria-label="每页谱面集数量"
                className="rounded-xl border border-white/[0.07] bg-[#111725] px-3 py-2 text-xs text-slate-300 outline-none"
                onChange={(event) => {
                  setLimit(Number(event.target.value));
                  setOffset(0);
                }}
                value={limit}
              >
                {[20, 40, 80, 120].map((value) => (
                  <option key={value} value={value}>
                    {value} / 页
                  </option>
                ))}
              </select>
            </div>
            <div className="flex gap-2">
              <Button
                disabled={offset === 0}
                onClick={() => setOffset(Math.max(0, offset - limit))}
                size="icon"
              >
                <ChevronLeft className="size-4" />
              </Button>
              <Button
                disabled={offset + limit >= sets.data.total}
                onClick={() => setOffset(offset + limit)}
                size="icon"
              >
                <ChevronRight className="size-4" />
              </Button>
            </div>
          </div>
        </>
      ) : (
        <EmptyState
          action={
            <Button onClick={reset}>
              <RotateCcw className="size-4" />
              清空筛选
            </Button>
          }
          icon={<ListFilter className="size-5" />}
          title="没有符合条件的谱面集"
          description="尝试放宽星数、BPM、长度或结构参数；搜索同时支持标题、艺术家、mapper、标签与在线 ID。"
        />
      )}
    </div>
  );
}
