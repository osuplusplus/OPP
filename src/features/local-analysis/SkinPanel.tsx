import { useDebouncedValue } from "../../shared/lib/useDebouncedValue";
import { useEffect, useMemo, useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import {
  ChevronLeft,
  ChevronRight,
  FileAudio2,
  FilePenLine,
  FolderSearch,
  PackageOpen,
  Image,
  Layers3,
  LoaderCircle,
  Palette,
  Play,
  SlidersHorizontal,
  Volume2,
  X,
} from "lucide-react";
import { ErrorPanel } from "../../shared/components/ErrorPanel";
import {
  Badge,
  Button,
  Card,
  EmptyState,
  Skeleton,
} from "../../shared/components/ui";
import { SearchAutocomplete } from "../../shared/components/SearchAutocomplete";
import { cn } from "../../shared/lib/cn";
import { dateTime, fullNumber } from "../../shared/lib/format";
import { desktopApi } from "../../shared/lib/tauri";
import type {
  LocalSkinAssetSummary,
  LocalSkinSummary,
  OsuClient,
  SkinQuery,
  SkinSort,
} from "../../shared/types/osu";
import {
  useLocalSkinAsset,
  useLocalSkinDetail,
  useLocalSkinPreview,
  useLocalSkins,
} from "./api";

const PAGE_SIZE = 24;
const SOUND_PAGE_SIZE = 40;

function formatBytes(value?: number | null) {
  if (value === null || value === undefined) return "—";
  if (value < 1024) return `${value} B`;
  const units = ["KB", "MB", "GB"];
  let size = value / 1024;
  let unit = units[0];
  for (let index = 1; size >= 1024 && index < units.length; index += 1) {
    size /= 1024;
    unit = units[index];
  }
  return `${size.toFixed(size >= 10 ? 1 : 2)} ${unit}`;
}

function SkinSwatches({ skin }: { skin: LocalSkinSummary }) {
  const colors = skin.accent_colors.slice(0, 5);
  return (
    <div className="flex -space-x-1.5">
      {colors.length ? (
        colors.map((color, index) => (
          <span
            className="size-5 rounded-full border-2 border-[#111725]"
            key={`${color.join("-")}-${index}`}
            style={{ backgroundColor: `rgb(${color.slice(0, 3).join(",")})` }}
          />
        ))
      ) : (
        <span className="size-5 rounded-full border-2 border-[#111725] bg-slate-700" />
      )}
    </div>
  );
}

function SkinListItem({ active, skin, onSelect }: { active: boolean; skin: LocalSkinSummary; onSelect: () => void }) {
  const [exporting, setExporting] = useState(false);
  const [exportNotice, setExportNotice] = useState<string | null>(null);
  const exportOsk = async () => {
    const outDir = await desktopApi.chooseDirectory("选择 .osk 导出位置");
    if (!outDir) return;
    setExporting(true);
    setExportNotice(null);
    try {
      const path = await desktopApi.exportLocalSkin(skin.resource.client, skin.resource.resource_id, outDir);
      setExportNotice(`已导出：${path}`);
    } catch (caught) {
      setExportNotice(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setExporting(false);
    }
  };
  return (
    <div className={cn("w-full rounded-xl border p-4 text-left transition-colors", active ? "border-[var(--theme-primary-soft)] bg-[var(--theme-primary-muted)]" : "border-white/[0.06] bg-white/[0.025] hover:border-white/[0.12] hover:bg-white/[0.045]")}>
      <button className="w-full text-left" onClick={onSelect} type="button">
        <div className="flex items-start gap-3">
          <div className="grid size-10 shrink-0 place-items-center rounded-xl border border-pink-300/15 bg-pink-300/10 text-pink-200"><Palette className="size-4" /></div>
          <div className="min-w-0 flex-1"><h3 className="truncate text-sm font-semibold text-white">{skin.name}</h3><p className="mt-1 truncate text-[11px] text-slate-500">{skin.author} · {skin.version}</p></div>
          <SkinSwatches skin={skin} />
        </div>
        <div className="mt-4 flex items-center justify-between text-[10px] text-slate-600"><span>{skin.resource_count === null ? "资源未知" : `${skin.resource_count} 个资源`}</span><span>{formatBytes(skin.total_bytes)}</span></div>
      </button>
      <div className="mt-3 flex gap-2">
        {skin.resource.client === "stable" ? (
          <>
            <Button aria-label="在资源管理器中打开 Skin" className="flex-1" disabled={!skin.resource.logical_path} onClick={() => { const path = skin.resource.logical_path; if (path) void desktopApi.openLocalResourceInExplorer(skin.resource.client, path); }} size="sm"><FolderSearch className="size-3.5" />打开 Skin 文件</Button>
            <Button aria-label="导出为 .osk 皮肤包" className="flex-1" disabled={exporting} loading={exporting} onClick={() => void exportOsk()} size="sm" variant="primary"><PackageOpen className="size-3.5" />导出 .osk</Button>
          </>
        ) : (
          // Lazer 皮肤文件按哈希散落存储，没有可打开的皮肤目录，直接提供导出。
          <Button aria-label="导出为 .osk 皮肤包" className="w-full" disabled={exporting} loading={exporting} onClick={() => void exportOsk()} size="sm" variant="primary"><PackageOpen className="size-3.5" />导出 .osk</Button>
        )}
      </div>
      {exportNotice ? <p className="mt-2 truncate text-[11px] text-slate-500" title={exportNotice}>{exportNotice}</p> : null}
    </div>
  );
}

function ImagePreview({
  asset,
  client,
  skinResourceId,
  onReplace,
}: {
  asset: LocalSkinAssetSummary;
  client: OsuClient;
  skinResourceId: string;
  onReplace: (asset: LocalSkinAssetSummary) => void;
}) {
  const query = useLocalSkinAsset(client, skinResourceId, asset.resource_id);
  return (
    <article className="group overflow-hidden rounded-2xl border border-white/[0.07] bg-black/15">
      <div className="grid aspect-[4/3] place-items-center bg-[linear-gradient(45deg,rgba(255,255,255,.04)_25%,transparent_25%),linear-gradient(-45deg,rgba(255,255,255,.04)_25%,transparent_25%),linear-gradient(45deg,transparent_75%,rgba(255,255,255,.04)_75%),linear-gradient(-45deg,transparent_75%,rgba(255,255,255,.04)_75%)] bg-[length:18px_18px] bg-[position:0_0,0_9px,9px_-9px,-9px_0px] p-4">
        {query.isLoading ? (
          <LoaderCircle className="size-5 animate-spin text-slate-600" />
        ) : query.data ? (
          <img
            alt={asset.name}
            className="max-h-full max-w-full object-contain [image-rendering:auto] transition group-hover:scale-105"
            loading="lazy"
            src={query.data.data_url}
          />
        ) : (
          <Image className="size-5 text-slate-700" />
        )}
      </div>
      <div className="border-t border-white/[0.055] px-3 py-2.5">
        <p className="truncate text-xs font-medium text-slate-200">{asset.name}</p>
        <div className="mt-1 flex justify-between text-[9px] uppercase tracking-wider text-slate-600">
          <span>{asset.category}</span>
          <span>{formatBytes(asset.size)}</span>
        </div>
        <Button className="mt-2 w-full" onClick={() => onReplace(asset)} size="sm"><FilePenLine className="size-3.5" />替换</Button>
      </div>
    </article>
  );
}

function SoundPreview({
  asset,
  client,
  skinResourceId,
  onReplace,
}: {
  asset: LocalSkinAssetSummary;
  client: OsuClient;
  skinResourceId: string;
  onReplace: (asset: LocalSkinAssetSummary) => void;
}) {
  const [requested, setRequested] = useState(false);
  const audioRef = useRef<HTMLAudioElement>(null);
  const query = useLocalSkinAsset(
    client,
    skinResourceId,
    asset.resource_id,
    requested,
  );
  useEffect(() => {
    if (query.data && audioRef.current) {
      audioRef.current.play().catch(() => undefined);
    }
  }, [query.data]);

  return (
    <div className="flex items-center gap-3 rounded-xl border border-white/[0.06] bg-white/[0.025] p-3">
      <Button
        aria-label={`试听 ${asset.name}`}
        loading={requested && query.isLoading}
        onClick={() => {
          if (query.data && audioRef.current) {
            audioRef.current.currentTime = 0;
            audioRef.current.play().catch(() => undefined);
          } else {
            setRequested(true);
          }
        }}
        size="icon"
      >
        <Play className="size-3.5" />
      </Button>
      <div className="min-w-0 flex-1">
        <p className="truncate text-xs font-medium text-slate-200">{asset.name}</p>
        <p className="mt-1 text-[10px] text-slate-600">
          {asset.category} · {asset.extension.toUpperCase()} · {formatBytes(asset.size)}
        </p>
      </div>
      {query.data ? (
        <audio controls className="h-8 w-56" ref={audioRef} src={query.data.data_url}>
          <track kind="captions" />
        </audio>
      ) : (
        <Volume2 className="size-4 text-slate-700" />
      )}
      <Button aria-label={`替换 ${asset.name}`} onClick={() => onReplace(asset)} size="icon"><FilePenLine className="size-3.5" /></Button>
    </div>
  );
}

function SkinWorkspace({
  client,
  resourceId,
  onAssetsReplaced,
}: {
  client: OsuClient;
  resourceId: string;
  onAssetsReplaced: () => Promise<void>;
}) {
  const detail = useLocalSkinDetail(client, resourceId);
  const preview = useLocalSkinPreview(client, resourceId);
  const [view, setView] = useState<"images" | "sounds" | "config">("images");
  const [imagePage, setImagePage] = useState(0);
  const [soundPage, setSoundPage] = useState(0);
  const [replacement, setReplacement] = useState<LocalSkinAssetSummary | null>(null);
  const [saveAsNew, setSaveAsNew] = useState(false);
  const [newSkinName, setNewSkinName] = useState("");
  const [replacing, setReplacing] = useState(false);
  const [replaceError, setReplaceError] = useState<unknown>(null);
  const skin = detail.data;
  const images = preview.data?.images ?? [];
  const imageSlice = images.slice(
    imagePage * PAGE_SIZE,
    (imagePage + 1) * PAGE_SIZE,
  );
  const sounds = preview.data?.sounds ?? [];
  const soundSlice = sounds.slice(
    soundPage * SOUND_PAGE_SIZE,
    (soundPage + 1) * SOUND_PAGE_SIZE,
  );

  if (detail.isLoading) return <Skeleton className="min-h-[560px]" />;
  if (detail.error || !skin) {
    return <ErrorPanel error={detail.error} onRetry={() => detail.refetch()} />;
  }

  const views = [
    ["images", "图片", Image, images.length],
    ["sounds", "音效", FileAudio2, preview.data?.sounds.length ?? 0],
    ["config", "配置", SlidersHorizontal, skin.sections.length],
  ] as const;
  const replaceAsset = async () => {
    if (!replacement || (saveAsNew && !newSkinName.trim())) return;
    setReplaceError(null);
    const selected = await desktopApi.chooseSkinAssetFile(replacement.extension);
    if (!selected) return;
    setReplacing(true);
    try {
      await desktopApi.replaceLocalSkinAsset(client, resourceId, replacement.resource_id, selected, saveAsNew, saveAsNew ? newSkinName.trim() : undefined);
      await onAssetsReplaced();
      setReplacement(null);
    } catch (error) {
      setReplaceError(error);
    } finally {
      setReplacing(false);
    }
  };
  const openReplacement = (asset: LocalSkinAssetSummary) => {
    setSaveAsNew(false);
    setNewSkinName("");
    setReplaceError(null);
    setReplacement(asset);
  };

  return (
    <>
    <Card className="min-w-0 overflow-hidden">
      <header className="relative overflow-hidden border-b border-white/[0.06] p-6">
        <div className="absolute inset-0 bg-[radial-gradient(circle_at_85%_10%,rgba(255,106,167,.13),transparent_36%),radial-gradient(circle_at_15%_95%,rgba(92,225,230,.1),transparent_35%)]" />
        <div className="relative flex items-start justify-between gap-5">
          <div className="min-w-0">
            <div className="mb-3 flex flex-wrap gap-2">
              <Badge tone={skin.summary.completeness === "complete" ? "success" : "warning"}>
                {skin.summary.completeness === "complete" ? "完整资源" : "配置索引"}
              </Badge>
              {skin.summary.has_mania_config ? <Badge tone="cyan">Mania</Badge> : null}
            </div>
            <h2 className="truncate text-2xl font-semibold tracking-tight text-white">
              {skin.summary.name}
            </h2>
            <p className="mt-2 text-sm text-slate-400">
              {skin.summary.author} · {skin.summary.version}
            </p>
          </div>
          <div className="flex shrink-0 items-center gap-6 text-right">
            <div>
              <p className="font-mono text-lg font-semibold text-white">
                {skin.inventory ? fullNumber(skin.inventory.file_count) : "—"}
              </p>
              <p className="mt-1 text-[10px] uppercase tracking-wider text-slate-600">Resources</p>
            </div>
            <div>
              <p className="font-mono text-lg font-semibold text-white">
                {formatBytes(skin.inventory?.total_bytes)}
              </p>
              <p className="mt-1 text-[10px] uppercase tracking-wider text-slate-600">Size</p>
            </div>
            <SkinSwatches skin={skin.summary} />
          </div>
        </div>
      </header>

      <nav className="flex items-center gap-1 border-b border-white/[0.06] bg-black/10 px-4 py-2">
        {views.map(([key, label, Icon, count]) => (
          <button
            className={cn(
              "flex items-center gap-2 rounded-xl px-3 py-2 text-xs font-semibold transition",
              view === key
                ? "bg-white/[0.08] text-white"
                : "text-slate-500 hover:bg-white/[0.04] hover:text-slate-300",
            )}
            key={key}
            onClick={() => setView(key)}
            type="button"
          >
            <Icon className="size-3.5" />
            {label}
            <span className="font-mono text-[10px] text-slate-600">{count}</span>
          </button>
        ))}
        <p className="ml-auto text-[10px] text-slate-700">
          更新于 {dateTime(skin.summary.modified_at)}
        </p>
      </nav>

      <div className="p-4">
        {preview.isLoading && view !== "config" ? (
          <div className="grid min-h-72 place-items-center">
            <LoaderCircle className="size-6 animate-spin text-cyan-200" />
          </div>
        ) : preview.error && view !== "config" ? (
          <ErrorPanel error={preview.error} onRetry={() => preview.refetch()} />
        ) : view === "images" ? (
          images.length ? (
            <>
              <div className="grid grid-cols-4 gap-3">
                {imageSlice.map((asset) => (
                  <ImagePreview
                    asset={asset}
                    client={client}
                    key={asset.resource_id}
                    onReplace={openReplacement}
                    skinResourceId={resourceId}
                  />
                ))}
              </div>
              {images.length > PAGE_SIZE ? (
                <div className="mt-4 flex items-center justify-between">
                  <p className="text-xs text-slate-600">
                    第 {imagePage + 1} / {Math.ceil(images.length / PAGE_SIZE)} 页
                  </p>
                  <div className="flex gap-2">
                    <Button
                      disabled={imagePage === 0}
                      onClick={() => setImagePage((page) => page - 1)}
                      size="icon"
                    >
                      <ChevronLeft className="size-4" />
                    </Button>
                    <Button
                      disabled={(imagePage + 1) * PAGE_SIZE >= images.length}
                      onClick={() => setImagePage((page) => page + 1)}
                      size="icon"
                    >
                      <ChevronRight className="size-4" />
                    </Button>
                  </div>
                </div>
              ) : null}
            </>
          ) : (
            <EmptyState
              icon={<Image className="size-5" />}
              title="没有可关联的图片"
              description={"这个 Skin 中没有支持预览的图片。"}
            />
          )
        ) : view === "sounds" ? (
          sounds.length ? (
            <>
              <div className="grid grid-cols-2 gap-2">
                {soundSlice.map((asset) => (
                  <SoundPreview
                    asset={asset}
                    client={client}
                    key={asset.resource_id}
                    onReplace={openReplacement}
                    skinResourceId={resourceId}
                  />
                ))}
              </div>
              {sounds.length > SOUND_PAGE_SIZE ? (
                <div className="mt-4 flex items-center justify-between">
                  <p className="text-xs text-slate-600">
                    第 {soundPage + 1} / {Math.ceil(sounds.length / SOUND_PAGE_SIZE)} 页
                  </p>
                  <div className="flex gap-2">
                    <Button
                      disabled={soundPage === 0}
                      onClick={() => setSoundPage((page) => page - 1)}
                      size="icon"
                    >
                      <ChevronLeft className="size-4" />
                    </Button>
                    <Button
                      disabled={(soundPage + 1) * SOUND_PAGE_SIZE >= sounds.length}
                      onClick={() => setSoundPage((page) => page + 1)}
                      size="icon"
                    >
                      <ChevronRight className="size-4" />
                    </Button>
                  </div>
                </div>
              ) : null}
            </>
          ) : (
            <EmptyState
              icon={<FileAudio2 className="size-5" />}
              title="没有可关联的音效"
              description={"这个 Skin 中没有 WAV、MP3 或 OGG 音效。"}
            />
          )
        ) : (
          <div className="space-y-2">
            {skin.sections.map((section, sectionIndex) => (
              <details
                className="group rounded-xl border border-white/[0.06] bg-white/[0.02]"
                key={`${section.name}-${sectionIndex}`}
                open={sectionIndex === 0}
              >
                <summary className="flex cursor-pointer list-none items-center gap-2 px-4 py-3 text-sm font-semibold text-slate-200">
                  <Layers3 className="size-3.5 text-cyan-200" />
                  [{section.name}]
                  <span className="ml-auto font-mono text-[10px] text-slate-600">
                    {section.entries.length}
                  </span>
                </summary>
                <div className="divide-y divide-white/[0.045] border-t border-white/[0.05] px-4">
                  {section.entries.map((entry, entryIndex) => (
                    <div
                      className="grid grid-cols-[minmax(120px,.4fr)_1fr] gap-4 py-2.5 text-xs"
                      key={`${entry.key}-${entryIndex}`}
                    >
                      <span className="font-mono text-slate-600">{entry.key}</span>
                      <span className="flex min-w-0 items-center gap-2 break-all text-slate-300">
                        {entry.color ? (
                          <span
                            className="size-4 shrink-0 rounded border border-white/20"
                            style={{ backgroundColor: `rgb(${entry.color.slice(0, 3).join(",")})` }}
                          />
                        ) : null}
                        {entry.value}
                      </span>
                    </div>
                  ))}
                </div>
              </details>
            ))}
          </div>
        )}
      </div>
    </Card>
    <Dialog.Root onOpenChange={(open) => { if (!open && !replacing) { setReplacement(null); setReplaceError(null); } }} open={Boolean(replacement)}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-[120] bg-black/65 backdrop-blur-sm" />
        <Dialog.Content className="fixed left-1/2 top-1/2 z-[130] w-[min(520px,calc(100vw-3rem))] -translate-x-1/2 -translate-y-1/2 rounded-2xl border border-white/10 bg-[#111725] p-6 shadow-2xl outline-none">
          <Dialog.Title className="text-xl font-semibold text-white">替换 Skin 资源</Dialog.Title>
          <Dialog.Description className="mt-2 text-sm leading-6 text-slate-400">将替换 {replacement?.name}（.{replacement?.extension}）。可直接替换当前 Skin，或先复制为新的 Skin 后再替换。</Dialog.Description>
          <Dialog.Close aria-label="关闭替换面板" className="absolute right-4 top-4 grid size-9 place-items-center rounded-xl text-slate-500 hover:bg-white/[0.06] hover:text-white"><X className="size-4" /></Dialog.Close>
          <label className="mt-6 flex items-center gap-3 rounded-xl border border-white/[0.08] bg-white/[0.025] p-4 text-sm text-slate-200"><input checked={saveAsNew} className="accent-[var(--theme-primary)]" onChange={(event) => setSaveAsNew(event.target.checked)} type="checkbox" />另存为新的 Skin</label>
          {saveAsNew ? <label className="mt-4 block text-sm font-medium text-slate-200">新 Skin 名称<input autoFocus className="mt-2 w-full rounded-xl border border-white/[0.1] bg-black/20 px-3 py-2.5 text-white outline-none focus:border-[var(--theme-primary)]" onChange={(event) => setNewSkinName(event.target.value)} placeholder="例如 My Skin - edit" value={newSkinName} /></label> : null}
          {replaceError ? <div className="mt-4"><ErrorPanel error={replaceError} onRetry={() => void replaceAsset()} /></div> : null}
          <div className="mt-6 flex justify-end gap-3"><Dialog.Close asChild><Button disabled={replacing} variant="ghost">取消</Button></Dialog.Close><Button disabled={replacing || (saveAsNew && !newSkinName.trim())} loading={replacing} onClick={() => void replaceAsset()} variant="primary">选择 .{replacement?.extension} 文件</Button></div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
    </>
  );
}

export function SkinPanel({ client }: { client: OsuClient }) {
  const [search, setSearch] = useState("");
  const deferredSearch = useDebouncedValue(search);
  const [sort, setSort] = useState<SkinSort>("name");
  const [direction, setDirection] = useState<"asc" | "desc">("asc");
  const [offset, setOffset] = useState(0);
  const [selected, setSelected] = useState<string | null>(null);
  const query = useMemo<SkinQuery>(
    () => ({
      client,
      search: deferredSearch,
      sort,
      direction,
      offset,
      limit: 100,
    }),
    [client, deferredSearch, direction, offset, sort],
  );
  const skins = useLocalSkins(query, true);

  const items = skins.data?.items ?? [];
  const searchSuggestions = items.flatMap((item) => [
    { value: item.name, detail: "名称" },
    { value: item.author, detail: "作者" },
    { value: item.version, detail: "版本" },
  ]);
  const activeResourceId = items.some(
    (skin) => skin.resource.resource_id === selected,
  )
    ? selected
    : (items[0]?.resource.resource_id ?? null);

  return (
    <div>
      <Card className="mb-4 flex items-center gap-3 p-3">
        <div className="min-w-64 flex-1">
          <SearchAutocomplete
            aria-label="搜索 Skin"
            className="w-full rounded-xl border border-white/[0.07] bg-black/20 py-2.5 pl-10 pr-4 text-sm text-white outline-none placeholder:text-slate-700 focus:border-cyan-300/25"
            onChange={(value) => {
              setSearch(value);
              setOffset(0);
            }}
            placeholder="搜索名称、作者或版本"
            suggestions={searchSuggestions}
            value={search}
          />
        </div>
        <select
          aria-label="Skin 排序字段"
          className="rounded-xl border border-white/[0.07] bg-[#111725] px-3 py-2.5 text-xs text-slate-300 outline-none"
          onChange={(event) => {
            setSort(event.target.value as SkinSort);
            setOffset(0);
          }}
          value={sort}
        >
          <option value="name">名称</option>
          <option value="author">作者</option>
          <option value="size">大小</option>
          <option value="modified_at">修改时间</option>
        </select>
        <select
          aria-label="Skin 排序方向"
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
        <Badge tone="pink">{skins.data ? `${fullNumber(skins.data.total)} 个 Skin` : "索引"}</Badge>
      </Card>

      {skins.isLoading ? (
        <Skeleton className="h-[620px]" />
      ) : skins.error ? (
        <ErrorPanel error={skins.error} onRetry={() => skins.refetch()} />
      ) : skins.data?.items.length ? (
        <>
          <div className="grid min-h-0 grid-cols-[280px_minmax(0,1fr)] items-start gap-4">
            <aside className="min-h-0 max-h-[calc(100vh-270px)] space-y-2 overflow-y-auto overscroll-contain pr-1">
              {skins.data.items.map((skin) => (
                <SkinListItem
                  active={activeResourceId === skin.resource.resource_id}
                  key={skin.resource.resource_id}
                  onSelect={() => setSelected(skin.resource.resource_id)}
                  skin={skin}
                />
              ))}
            </aside>
            {activeResourceId ? (
              <div className="min-h-0 min-w-0 max-h-[calc(100vh-270px)] overflow-y-auto overscroll-contain pr-1">
                <SkinWorkspace
                  client={client}
                  key={activeResourceId}
                  onAssetsReplaced={async () => { setSelected(null); await skins.refetch(); }}
                  resourceId={activeResourceId}
                />
              </div>
            ) : null}
          </div>
          {skins.data.total > skins.data.limit ? (
            <div className="mt-4 flex items-center justify-between">
              <p className="text-xs text-slate-600">
                第 {Math.floor(offset / skins.data.limit) + 1} /{" "}
                {Math.ceil(skins.data.total / skins.data.limit)} 页
              </p>
              <div className="flex gap-2">
                <Button
                  disabled={offset === 0}
                  onClick={() => setOffset(Math.max(0, offset - skins.data!.limit))}
                  size="icon"
                >
                  <ChevronLeft className="size-4" />
                </Button>
                <Button
                  disabled={offset + skins.data.limit >= skins.data.total}
                  onClick={() => setOffset(offset + skins.data!.limit)}
                  size="icon"
                >
                  <ChevronRight className="size-4" />
                </Button>
              </div>
            </div>
          ) : null}
        </>
      ) : (
        <EmptyState
          icon={<Palette className="size-5" />}
          title="没有匹配的 Skin"
          description={"当前索引中没有匹配的皮肤。"}
        />
      )}
    </div>
  );
}
