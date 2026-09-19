import { memo, useEffect, useRef, useState } from "react";
import { Music2, Search, Trash2, X } from "lucide-react";
import { musicApi, musicError } from "./api";
import type { MusicQueuePage, MusicState } from "../../shared/types/music";

const ROW = 48;
const PAGE = 50;
export const MusicQueue = memo(function MusicQueue({ state, onError }: { state: MusicState; onError: (message: string) => void }) {
  const [search, setSearch] = useState("");
  const [view, setView] = useState({ query: "", offset: 0 });
  const [loaded, setLoaded] = useState<{ data: MusicQueuePage; offset: number; query: string } | null>(null);
  const page = loaded?.offset === view.offset && loaded.query === view.query && loaded.data.version === state.queue_version ? loaded.data : null;
  // Keep the scroll extent while the next page is loading; removing it resets scrollTop to zero.
  const total = page?.total ?? (view.query && loaded?.query === view.query ? loaded.data.total : state.total);
  const scroll = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const timer = window.setTimeout(() => { setView({ query: search, offset: 0 }); if (scroll.current) scroll.current.scrollTop = 0; }, 180);
    return () => window.clearTimeout(timer);
  }, [search]);
  useEffect(() => {
    let active = true;
    void musicApi.queue(view.offset, PAGE + 12, view.query).then((result) => {
      if (!active || result.version < state.queue_version) return;
      if (view.offset >= result.total && view.offset > 0) {
        const offset = Math.floor(Math.max(0,result.total - 1) / PAGE) * PAGE;
        setView({ ...view, offset });
        if (scroll.current) scroll.current.scrollTop = offset * ROW;
      } else setLoaded({ data: result, ...view });
    })
      .catch((error) => { if (active) onError(musicError(error)); });
    return () => { active = false; };
  }, [view, state.queue_version, onError]);
  const run = (action: "select" | "remove", id: string) => { void musicApi.control({ action, id }).catch((error) => onError(musicError(error))); };
  return <div className="music-queue">
    <div className="music-queue-heading"><span>播放列表 <b>{state.total}</b></span><button aria-label="清空播放列表" disabled={!state.total} onClick={() => void musicApi.control({ action: "clear" }).catch((e) => onError(musicError(e)))}><Trash2 /></button></div>
    <label className="music-search"><Search /><input aria-label="搜索播放列表" placeholder="搜索歌名、艺术家" value={search} onChange={(event) => setSearch(event.target.value)} /></label>
    <div className="music-queue-scroll" ref={scroll} onScroll={(event) => {
      const offset = Math.floor(event.currentTarget.scrollTop / ROW / PAGE) * PAGE;
      if (offset !== view.offset) setView({ ...view, offset });
    }}>
      {total > 0 ? <div style={{ height: total * ROW, position: "relative" }}>
        <div style={{ position: "absolute", top: view.offset * ROW, width: "100%" }}>
          {page?.items.map((track) => <div className={`music-queue-row${state.current?.id === track.id ? " is-current" : ""}`} key={track.id}>
            <button className="music-track-pick" onClick={() => run("select", track.id)} aria-label={`播放 ${track.title}`} aria-pressed={state.current?.id === track.id}>
              <Music2 /><span><strong>{track.title}</strong><small>{track.artist} · {track.clients.join(" / ")}</small></span>
            </button><button aria-label={`移除 ${track.title}`} onClick={() => run("remove", track.id)}><X /></button>
          </div>)}
        </div>
      </div> : <p className="music-empty">{!page ? "正在读取播放列表…" : state.total ? "没有匹配的歌曲" : "从本地谱库加入喜欢的歌曲"}</p>}
    </div>
  </div>;
}, (a,b) => a.onError === b.onError && a.state.queue_version === b.state.queue_version && a.state.total === b.state.total && a.state.current?.id === b.state.current?.id);
