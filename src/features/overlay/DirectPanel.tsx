import { useEffect, useRef, useState } from "react";
import { desktopApi } from "../../shared/lib/tauri";
import { errorMessage } from "../../shared/lib/format";
import type {
  AppSettings,
  BeatmapDownloadProgress,
  LocalBeatmapPresence,
  OnlineBeatmapset,
  OverlayMpActivity,
  Ruleset,
  TosuLiveSnapshot,
} from "../../shared/types/osu";
import { createDefaultSearchQuery, starRange } from "../online-beatmaps/filters";
import { displayTitle, isMultiplayerState, resolveDownloadProvider } from "./model";

const SEARCH_DEBOUNCE_MS = 400;
const PRESENCE_CHUNK = 1000;

type SetPresence = "present" | "partial" | "missing" | "unknown";

function summarizeSetPresence(
  set: OnlineBeatmapset,
  presence: Map<number, LocalBeatmapPresence>,
): SetPresence {
  const maps = set.beatmaps ?? [];
  if (!maps.length) return "unknown";
  const statuses = maps.map((map) => presence.get(map.id)?.status ?? "unknown");
  if (statuses.every((status) => status === "present")) return "present";
  if (statuses.some((status) => status === "present")) return "partial";
  if (statuses.every((status) => status === "missing")) return "missing";
  return "unknown";
}

function mpSummary(activity: OverlayMpActivity): string | null {
  switch (activity.phase) {
    case "idle":
      return null;
    case "checking":
      return `检查本地谱面：${activity.title}`;
    case "missing":
      return `缺少谱面：${activity.title}`;
    case "downloading":
      return `MP 补缺下载：${activity.title}`;
    case "imported":
      return `已下载并导入：${activity.title}`;
    case "present":
      return `已有谱面：${activity.title}`;
    case "unresolved":
      return `无法识别谱面：${activity.title}`;
    case "error":
      return activity.message;
    default:
      return null;
  }
}

export function DirectPanel({
  settings,
  snapshot,
  mpActivity,
  download,
  onClose,
}: {
  settings: AppSettings | null;
  snapshot: TosuLiveSnapshot | null;
  mpActivity: OverlayMpActivity;
  download: BeatmapDownloadProgress | null;
  onClose: () => void;
}) {
  const [text, setText] = useState("");
  const [ruleset, setRuleset] = useState<Ruleset>("osu");
  const [status, setStatus] = useState("ranked");
  const [items, setItems] = useState<OnlineBeatmapset[]>([]);
  const [cursor, setCursor] = useState<string | null | undefined>(undefined);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [presence, setPresence] = useState<Map<number, LocalBeatmapPresence>>(new Map());
  const searchSeq = useRef(0);

  const searchPage = async (pageCursor: string | null, replace: boolean) => {
    const seq = searchSeq.current + 1;
    searchSeq.current = seq;
    setLoading(true);
    setError(null);
    try {
      const query = createDefaultSearchQuery(ruleset);
      query.query = text.trim();
      query.status = status;
      query.sort = text.trim() ? "relevance_desc" : "ranked_desc";
      query.cursor_string = pageCursor;
      const response = await desktopApi.searchOnlineBeatmapsets(query);
      if (searchSeq.current !== seq) return;
      setItems((previous) => (replace ? response.beatmapsets : [...previous, ...response.beatmapsets]));
      setCursor(response.cursor_string ?? undefined);
    } catch (caught) {
      if (searchSeq.current !== seq) return;
      setError(errorMessage(caught));
      if (replace) setItems([]);
    } finally {
      if (searchSeq.current === seq) setLoading(false);
    }
  };

  useEffect(() => {
    const timer = setTimeout(() => void searchPage(null, true), SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [text, ruleset, status]);

  useEffect(() => {
    // 空结果时不清空旧映射：当前列表为空不会查询，且新结果以异步覆盖。
    if (!items.length) return;
    let disposed = false;
    const ids = items.flatMap((set) => (set.beatmaps ?? []).map((map) => map.id));
    const chunks: number[][] = [];
    for (let index = 0; index < ids.length; index += PRESENCE_CHUNK) {
      chunks.push(ids.slice(index, index + PRESENCE_CHUNK));
    }
    void Promise.all(
      chunks.map((chunk) => desktopApi.queryLocalBeatmapPresence(chunk, null).catch(() => [])),
    ).then((results) => {
      if (disposed) return;
      setPresence(new Map(results.flat().map((entry) => [entry.beatmap_id, entry])));
    });
    return () => {
      disposed = true;
    };
  }, [items]);

  const downloadSet = async (set: OnlineBeatmapset) => {
    setError(null);
    try {
      const destination = settings?.beatmap_download_directory?.trim() ?? "";
      if (!destination) {
        setError("尚未配置谱面下载目录，请先在 OPP 主窗口设置中选择。");
        return;
      }
      await desktopApi.downloadOnlineBeatmapsets({
        destination,
        provider: resolveDownloadProvider(settings),
        overwrite: false,
        include_video: settings?.include_video_in_beatmap_downloads ?? true,
        open_after_download: true,
        items: [{ beatmapset_id: set.id, artist: set.artist, title: set.title }],
      });
    } catch (caught) {
      setError(errorMessage(caught));
    }
  };

  const downloadingId = download?.current_beatmapset_id ?? null;
  const mpLine = mpSummary(mpActivity);
  const hotkey = settings?.overlay_hotkey_toggle ?? "Control+Shift+O";

  return (
    <div className="panel">
      <header className="panel-head" data-tauri-drag-region>
        <span className="panel-title">OPP Direct</span>
        <span className="panel-hotkey">{hotkey} 收起</span>
        <button type="button" className="panel-close" onClick={onClose} aria-label="收起面板">
          —
        </button>
      </header>
      <div className="panel-controls">
        <input
          className="panel-search"
          value={text}
          placeholder="搜索谱面（需在 OPP 登录）"
          onChange={(event) => setText(event.target.value)}
          autoFocus
          spellCheck={false}
        />
        <select className="panel-select" value={ruleset} onChange={(event) => setRuleset(event.target.value as Ruleset)}>
          <option value="osu">osu!</option>
          <option value="taiko">taiko</option>
          <option value="fruits">catch</option>
          <option value="mania">mania</option>
        </select>
        <select className="panel-select" value={status} onChange={(event) => setStatus(event.target.value)}>
          <option value="any">全部状态</option>
          <option value="ranked">上架</option>
          <option value="loved">Loved</option>
          <option value="qualified">过审</option>
          <option value="pending">待定</option>
          <option value="graveyard">坟场</option>
        </select>
      </div>
      <div className="panel-results">
        {loading && items.length === 0 ? <div className="panel-hint">搜索中…</div> : null}
        {!loading && items.length === 0 && !error ? (
          <div className="panel-hint">输入关键词搜索，或浏览最新上架谱面。</div>
        ) : null}
        {error ? <div className="panel-error">{error}</div> : null}
        {items.map((set) => {
          const presenceStatus = summarizeSetPresence(set, presence);
          const busy = downloadingId === set.id;
          return (
            <div key={set.id} className="panel-item">
              <div className="panel-item-main">
                <div className="panel-item-title" title={displayTitle(set.artist_unicode ?? set.artist, set.title_unicode ?? set.title)}>
                  {displayTitle(set.artist_unicode ?? set.artist, set.title_unicode ?? set.title)}
                </div>
                <div className="panel-item-meta">
                  <span>{set.creator}</span>
                  <span>{starRange(set.beatmaps)}</span>
                  <span className={`presence presence-${presenceStatus}`}>
                    {presenceStatus === "present" ? "已有" : presenceStatus === "partial" ? "部分" : presenceStatus === "missing" ? "缺失" : "未知"}
                  </span>
                </div>
              </div>
              <button
                type="button"
                className="panel-download"
                disabled={busy || set.availability?.download_disabled}
                onClick={() => void downloadSet(set)}
              >
                {busy ? "下载中" : "下载"}
              </button>
            </div>
          );
        })}
        {cursor ? (
          <button type="button" className="panel-more" disabled={loading} onClick={() => void searchPage(cursor, false)}>
            {loading ? "加载中…" : "加载更多"}
          </button>
        ) : null}
      </div>
      <footer className="panel-foot">
        {download ? (
          <div className="panel-download-line" title={download.current_title ?? undefined}>
            下载 {download.processed}/{download.total}
            {download.current_title ? ` · ${download.current_title}` : ""}
          </div>
        ) : null}
        {mpLine || (isMultiplayerState(snapshot?.state) ? `多人状态：${snapshot?.state}` : null) ? (
          <div className="panel-mp-line">{mpLine ?? `多人状态：${snapshot?.state}`}</div>
        ) : null}
      </footer>
    </div>
  );
}
