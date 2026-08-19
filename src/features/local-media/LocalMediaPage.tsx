import { useCallback, useEffect, useMemo, useState } from "react";
import { Clipboard, FileVideo, Film, FolderSearch, Image, Images, RefreshCw } from "lucide-react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { useMode } from "../../app/ModeContext";
import { ErrorPanel } from "../../shared/components/ErrorPanel";
import { PageHeader } from "../../shared/components/PageHeader";
import { Badge, Button, Card, EmptyState, SectionTitle } from "../../shared/components/ui";
import { SearchAutocomplete } from "../../shared/components/SearchAutocomplete";
import { desktopApi } from "../../shared/lib/tauri";
import { APP_TIME_ZONE } from "../../shared/lib/format";
import type { GameMediaItem, GameReplayPayload, GameScreenshotPayload } from "../../shared/types/osu";

function formatBytes(value: number) {
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / 1024 / 1024).toFixed(1)} MB`;
}

function fileName(path: string) {
  return path.split(/[\\/]/).pop() ?? path;
}

function formatDate(value: string | null) {
  if (!value) return "未知时间";
  return new Intl.DateTimeFormat("zh-CN", { dateStyle: "medium", timeStyle: "short", timeZone: APP_TIME_ZONE }).format(new Date(value));
}

function base64ToBlob(base64: string, mimeType: string) {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
  return new Blob([bytes], { type: mimeType });
}

type Payload = GameReplayPayload | GameScreenshotPayload;
type MediaFilter = "all" | "screenshot" | "replay";

const mediaFilters: Array<{ value: MediaFilter; label: string; icon: typeof Images }> = [
  { value: "all", label: "全部", icon: Images },
  { value: "screenshot", label: "截图", icon: Image },
  { value: "replay", label: "回放", icon: FileVideo },
];

export function LocalMediaPage() {
  const { client } = useMode();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const requestedFilter = searchParams.get("type");
  const filter: MediaFilter = requestedFilter === "screenshot" || requestedFilter === "replay"
    ? requestedFilter
    : "all";
  const [items, setItems] = useState<GameMediaItem[]>([]);
  const [selected, setSelected] = useState<GameMediaItem | null>(null);
  const [payload, setPayload] = useState<Payload | null>(null);
  const [loading, setLoading] = useState(false);
  const [scanning, setScanning] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [mediaSearch, setMediaSearch] = useState("");

  const read = useCallback(async (item: GameMediaItem) => {
    setSelected(item);
    setPayload(null);
    setNotice(null);
    setLoading(true);
    try {
      setPayload(item.kind === "replay"
        ? await desktopApi.readGameReplay(client, item.path)
        : await desktopApi.readGameScreenshot(client, item.path));
    } catch (value) {
      setError(value);
    } finally {
      setLoading(false);
    }
  }, [client]);

  const filteredItems = useMemo(() => {
    const query = mediaSearch.trim().toLocaleLowerCase();
    return items.filter((item) => (filter === "all" || item.kind === filter)
      && (!query || `${fileName(item.path)} ${item.path}`.toLocaleLowerCase().includes(query)));
  }, [filter, items, mediaSearch]);
  const mediaSuggestions = useMemo(() => items.map((item) => ({
    value: fileName(item.path),
    label: fileName(item.path),
    detail: item.kind === "replay" ? "回放" : "截图",
  })), [items]);

  const refresh = useCallback(async () => {
    setScanning(true);
    setError(null);
    setSelected(null);
    setPayload(null);
    try {
      const nextItems = await desktopApi.listGameMedia(client);
      setItems(nextItems);
      const preferred = nextItems[0] ?? null;
      if (preferred) await read(preferred);
    } catch (value) {
      setError(value);
      setItems([]);
    } finally {
      setScanning(false);
    }
  }, [client, read]);

  useEffect(() => {
    const initial = window.setTimeout(() => void refresh(), 0);
    return () => window.clearTimeout(initial);
  }, [refresh]);

  const chooseFilter = (nextFilter: MediaFilter) => {
    const next = new URLSearchParams(searchParams);
    if (nextFilter === "all") next.delete("type");
    else next.set("type", nextFilter);
    setSearchParams(next, { replace: true });
  };

  const copyPath = async () => {
    if (!selected) return;
    try {
      await navigator.clipboard.writeText(selected.path);
      setNotice("文件路径已复制");
    } catch (value) {
      setError(value);
    }
  };

  const copyImage = async () => {
    if (!payload || !("mime_type" in payload)) return;
    try {
      const blob = base64ToBlob(payload.bytes_base64, payload.mime_type);
      await navigator.clipboard.write([new ClipboardItem({ [payload.mime_type]: blob })]);
      setNotice("图片已复制到剪贴板");
    } catch (value) {
      setError(value);
    }
  };

  const openExplorer = async () => {
    if (!selected) return;
    try {
      await desktopApi.openMediaInExplorer(client, selected.path);
      setNotice("已在资源管理器中定位文件");
    } catch (value) {
      setError(value);
    }
  };

  return (
    <div className="local-media-workspace flex h-[calc(100vh-104px-4rem)] min-h-0 flex-col overflow-hidden">
      <PageHeader
        eyebrow={`Local media · ${client}`}
        title="截图与回放"
        description="在同一个工作区浏览、预览和管理本地媒体"
        actions={<Button aria-label="刷新媒体" disabled={scanning} loading={scanning} onClick={() => void refresh()} size="icon"><RefreshCw className="size-4" /></Button>}
      />

      <div aria-label="媒体类型" className="mb-5 flex gap-2 border-b border-white/[0.06] pb-3" role="group">
        {mediaFilters.map(({ value, label, icon: Icon }) => (
          <button
            aria-pressed={filter === value}
            className={`inline-flex min-h-10 items-center gap-2 rounded-lg border px-4 text-sm font-medium transition-colors ${
              filter === value
                ? "selected-mask border-[var(--theme-primary)] text-[var(--theme-primary)]"
                : "border-transparent text-slate-500 hover:bg-white/[0.04] hover:text-slate-200"
            }`}
            key={value}
            onClick={() => chooseFilter(value)}
            type="button"
          >
            <Icon className="size-4" />
            {label}
          </button>
        ))}
      </div>

      {error ? <div className="mb-5"><ErrorPanel error={error} onRetry={() => void refresh()} /></div> : null}
      <div className="grid min-h-0 flex-1 grid-cols-[minmax(250px,300px)_minmax(0,1fr)] gap-5">
        <Card className="flex min-h-0 flex-col overflow-hidden p-3">
          <div className="flex items-center justify-between px-2 py-2">
            <SectionTitle title="本地媒体" description={`${filteredItems.length} 个对象`} />
            <Badge tone="cyan">{client}</Badge>
          </div>
          <SearchAutocomplete aria-label="搜索截图与回放" className="px-2" inputClassName="opp-input w-full py-2.5 pl-10 pr-3 text-sm" onChange={setMediaSearch} placeholder="搜索文件名或路径" suggestions={mediaSuggestions} value={mediaSearch} />
          <div className="mt-2 min-h-0 flex-1 space-y-2 overflow-y-auto pr-1">
            {scanning ? <div className="grid min-h-32 place-items-center text-sm text-slate-400"><span className="flex items-center gap-2"><RefreshCw className="size-4 animate-spin" />正在扫描本地媒体…</span></div> : filteredItems.length ? filteredItems.map((item) => (
              <button
                aria-pressed={selected?.path === item.path}
                className={`w-full rounded-xl border p-3 text-left transition-colors ${
                  selected?.path === item.path
                    ? "selected-mask border-[var(--theme-primary)]"
                    : "border-white/[0.06] bg-white/[0.02] hover:border-white/[0.14] hover:bg-white/[0.04]"
                }`}
                key={item.path}
                onClick={() => void read(item)}
                type="button"
              >
                <div className="flex items-center gap-2">
                  {item.kind === "screenshot" ? <Image className="size-4 shrink-0 text-cyan-300" /> : <FileVideo className="size-4 shrink-0 text-pink-300" />}
                  <p className="truncate text-sm font-medium text-slate-200">{fileName(item.path)}</p>
                </div>
                <p className="mt-2 text-sm text-slate-400">{formatBytes(item.size)} · {formatDate(item.modified_at)}</p>
              </button>
            )) : <EmptyState title="暂无媒体" description="在 osu! 中生成截图或回放后点击刷新。" />}
          </div>
        </Card>

        <Card className="flex min-h-0 min-w-0 flex-col overflow-hidden p-6">
          {selected ? <>
            <div className="flex flex-wrap items-start justify-between gap-4 border-b border-white/[0.06] pb-5">
              <div className="min-w-0">
                <div className="flex items-center gap-2">
                  {selected.kind === "screenshot" ? <Image className="size-5 text-cyan-300" /> : <FileVideo className="size-5 text-pink-300" />}
                  <h2 className="truncate text-xl font-semibold text-white">{fileName(selected.path)}</h2>
                </div>
                <p className="mt-2 truncate font-mono text-sm text-slate-400">{selected.path}</p>
                <p className="mt-1 text-sm text-slate-400">{formatBytes(selected.size)} · {formatDate(selected.modified_at)}</p>
              </div>
              <div className="flex flex-wrap gap-2">
                <Button onClick={() => void openExplorer()} size="sm"><FolderSearch className="size-4" />资源管理器</Button>
                <Button onClick={() => void copyPath()} size="sm"><Clipboard className="size-4" />复制路径</Button>
                {selected.kind === "replay" ? <Button onClick={() => navigate(`/local/media/render?replay=${encodeURIComponent(selected.path)}`)} size="sm" variant="primary"><Film className="size-4" />加入渲染</Button> : null}
                {payload && "mime_type" in payload ? <Button onClick={() => void copyImage()} size="sm"><Image className="size-4" />复制图片</Button> : null}
              </div>
            </div>
            {notice ? <p className="mt-4 rounded-lg border border-cyan-300/15 bg-cyan-300/[0.06] px-3 py-2 text-sm text-cyan-100">{notice}</p> : null}
            <div className="flex min-h-0 flex-1 items-center justify-center overflow-auto py-6">
              {loading
                ? <div className="text-sm text-slate-500">正在读取…</div>
                : payload && "mime_type" in payload
                  ? <img className="block h-auto max-h-full max-w-full shrink-0 rounded-xl border border-white/[0.06] object-contain" src={`data:${payload.mime_type};base64,${payload.bytes_base64}`} alt={payload.file_name} />
                  : payload
                    ? <div className="w-full max-w-2xl rounded-2xl border border-pink-300/15 bg-pink-300/[0.05] p-8"><FileVideo className="size-8 text-pink-200" /><h3 className="mt-4 text-lg font-semibold text-white">回放文件已读取</h3><p className="mt-2 text-sm text-slate-400">{payload.note}</p><p className="mt-4 font-mono text-sm text-slate-500">原始数据大小：{formatBytes(Math.ceil(payload.bytes_base64.length * 0.75))}</p></div>
                    : <div className="text-sm text-slate-500">选择对象以开始预览</div>}
            </div>
          </> : <div className="grid flex-1 place-items-center"><EmptyState title="选择一个对象" description="从左侧列表选择截图或回放，预览将在这里显示。" /></div>}
        </Card>
      </div>
    </div>
  );
}
