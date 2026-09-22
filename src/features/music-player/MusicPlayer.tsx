import { useCallback, useEffect, useState, type CSSProperties, type MouseEvent } from "react";
import { ChevronDown, ChevronUp, ListMusic, Maximize2, Minimize2, Minus, Music2, Pause, Pin, PinOff, Play, Plus, SkipBack, SkipForward, Volume2, X } from "lucide-react";
import type { BeatmapQuery, OsuClient } from "../../shared/types/osu";
import type { MusicMode, MusicQueueRequest } from "../../shared/types/music";
import { useArtworkPalette } from "../../shared/lib/stageArtwork";
import { MusicQueue } from "./MusicQueue";
import { musicApi, musicError, musicTime, useMusicState, useMusicControls } from "./api";
import "./musicPlayer.css";

export function MusicPlayer({ mini = false, client, resourceId, query }: { mini?: boolean; client?: OsuClient; resourceId?: string; query?: BeatmapQuery }) {
  const state = useMusicControls();
  const [expanded, setExpanded] = useState(false);
  const [opened, setOpened] = useState(false);
  const [pinned, setPinned] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [cover, setCover] = useState<{ id: string; source: string | null } | null>(null);
  const [source, setSource] = useState("all");
  const [collections, setCollections] = useState<{ id: string; name: string }[]>([]);
  const artwork = useArtworkPalette(cover?.id === state.current?.id ? cover?.source ?? null : null);
  const report = useCallback((message: string) => setError(message), []);
  const run = async (action: () => Promise<unknown>) => {
    setError(null); setBusy(true);
    try { await action(); } catch (error) { setError(musicError(error)); } finally { setBusy(false); }
  };
  const id = state.current?.id;
  useEffect(() => {
    if (!id) return;
    let active = true;
    void musicApi.artwork(id).then((source) => { if (active) setCover({ id, source }); }).catch(() => {});
    return () => { active = false; };
  }, [id]);
  useEffect(() => {
    let active = true; let dispose: (() => void) | undefined;
    void musicApi.onWindowError((message) => { if (active) { setError(message); setBusy(false); } }).then((stop) => { if (active) dispose = stop; else stop(); });
    return () => { active = false; dispose?.(); };
  }, []);
  useEffect(() => {
    if (!mini) return;
    let active = true;
    void musicApi.ready().then((session) => { if (active) setPinned(session.pinned ?? true); }).catch((error) => { if (active) setError(musicError(error)); });
    return () => { active = false; };
  }, [mini]);
  useEffect(() => {
    if (!expanded || mini || !musicApi.available()) return;
    let active = true;
    void musicApi.collections().then((result) => { if (active) setCollections(result.folders); }).catch((error) => { if (active) setError(musicError(error)); });
    return () => { active = false; };
  }, [expanded, mini]);
  const expand = () => void run(async () => {
    if (mini) await musicApi.layout(!expanded,pinned);
    if (!expanded) setOpened(true);
    setExpanded(!expanded);
  });
  const chooseSource = () => {
    const request: MusicQueueRequest = source === "results" && query ? { query }
      : source.startsWith("collection:") ? { collection_id: source.slice(11) }
      : source === "stable" || source === "lazer" ? { client: source } : {};
    void run(() => musicApi.setQueue(request));
  };
  const dragMini = (event: MouseEvent<HTMLDivElement>) => {
    if (!mini || event.button !== 0 || (event.target as Element).closest("button, input, select, .music-progress, .music-mini-actions")) return;
    event.preventDefault();
    void musicApi.drag().catch((error) => setError(musicError(error)));
  };
  return <section className={`music-player${mini ? " music-mini" : " music-docked"}${expanded ? " is-expanded" : ""}`} style={artwork.style} aria-label={mini ? "迷你音乐播放器" : "本地音乐播放器"}>
    {mini && artwork.source && <div className="music-artwork-backdrop" style={{ backgroundImage: `url("${artwork.source}")` }} aria-hidden="true" />}
    <div className="music-now" onMouseDown={dragMini}>
      <div className="music-cover">{artwork.source ? <img src={artwork.source} alt="" draggable={false} /> : <Music2 />}</div>
      <div className="music-copy"><strong title={state.current?.title}>{state.current?.title || "本地音乐"}</strong><span>{state.current?.artist || "让你的谱库继续播放"}</span></div>
      <div className="music-transport">
        <button aria-label="上一首" disabled={!state.total || busy} onClick={() => void run(() => musicApi.control({ action: "previous" }))}><SkipBack /></button>
        <button className="music-play" aria-label={state.playing ? "暂停音乐" : "播放音乐"} disabled={busy} onClick={() => void run(() => state.total ? musicApi.control({ action: "toggle" }) : musicApi.setQueue(resourceId ? { client, resource_id: resourceId } : {}))}>{state.playing ? <Pause /> : <Play />}</button>
        <button aria-label="下一首" disabled={!state.total || busy} onClick={() => void run(() => musicApi.control({ action: "next" }))}><SkipForward /></button>
      </div>
      {mini ? <div className="music-mini-actions">
        <button aria-label={pinned ? "取消置顶" : "窗口置顶"} aria-pressed={pinned} onClick={() => void run(async () => { await musicApi.layout(expanded,!pinned); setPinned(!pinned); })}>{pinned ? <Pin /> : <PinOff />}</button>
        <button aria-label="恢复完整界面" onClick={() => void run(() => musicApi.mode(false))}><Maximize2 /></button>
        <button aria-label="收起到托盘" onClick={() => void run(musicApi.hide)}><Minus /></button>
        <button aria-label="退出播放器" onClick={() => void run(musicApi.exit)}><X /></button>
      </div> : <button aria-label="切换迷你播放器" title="切换迷你播放器并释放完整界面" disabled={busy} onClick={() => void run(() => musicApi.mode(true))}><Minimize2 /></button>}
      <button className="music-list-toggle" aria-label={expanded ? "收起播放列表" : "展开播放列表"} aria-expanded={expanded} onClick={expand}>{mini ? expanded ? <ChevronUp /> : <ChevronDown /> : <ListMusic />}</button>
      <MusicProgress onError={report} />
    </div>
    {(error || state.notice || state.background_tasks > 0) && <p className="music-notice" role={error ? "alert" : "status"} title={error || state.notice || "已有后台任务继续执行，完成后释放资源"}>{error || state.notice || `${state.background_tasks} 项后台任务继续运行`}</p>}
    <div className="music-expanded" aria-hidden={!expanded} inert={!expanded}>
      {opened && <>
      <div className="music-options">
        <select aria-label="播放方式" value={state.mode} onChange={(e) => void run(() => musicApi.control({ action: "mode", mode: e.target.value as MusicMode }))}>
          <option value="sequential">顺序播放</option><option value="repeat_all">列表循环</option><option value="repeat_one">单曲循环</option><option value="shuffle">随机播放</option>
        </select>
        <Volume2 /><input aria-label="音乐音量" type="range" min={0} max={1} step={.01} value={state.volume} onChange={(e) => void run(() => musicApi.control({ action: "volume", value: Number(e.target.value) }))} />
      </div>
      {!mini && <div className="music-sources">
        <select aria-label="歌曲来源" value={source} onChange={(e) => setSource(e.target.value)}><option value="all">全部本地库 · 所有模式</option><option value="stable">Stable 谱库</option><option value="lazer">lazer 谱库</option>{query && <option value="results">当前搜索与筛选结果</option>}{collections.map((folder) => <option value={`collection:${folder.id}`} key={folder.id}>收藏夹 · {folder.name}</option>)}</select>
        <button disabled={busy} onClick={chooseSource} aria-label="播放所选来源"><Play />播放</button>
        {resourceId && <button disabled={busy} onClick={() => void run(() => musicApi.setQueue({ client, resource_id: resourceId, append: true }))} aria-label="将当前谱面加入播放列表"><Plus />当前歌曲</button>}
      </div>}
      <MusicQueue state={state} onError={report} />
      </>}
    </div>
  </section>;
}

function MusicProgress({ onError }: { onError: (message: string) => void }) {
  const state = useMusicState();
  const [seeking, setSeeking] = useState<number | null>(null);
  const progress = state.duration > 0 ? Math.min(100, Math.max(0, state.position / state.duration * 100)) : 0;
  return (      <div className="music-progress" style={{ "--music-progress": `${seeking === null ? progress : state.duration ? seeking / state.duration * 100 : 0}%` } as CSSProperties}>
        <span>{musicTime(seeking ?? state.position)}</span>
        <input aria-label="音乐播放进度" type="range" min={0} max={state.duration || 1} step={.5} value={seeking ?? Math.min(state.position,state.duration || 1)} disabled={!state.duration} onChange={(e) => setSeeking(Number(e.target.value))} onPointerUp={(e) => { void musicApi.control({ action: "seek", seconds: Number(e.currentTarget.value) }).catch((error) => onError(musicError(error))); setSeeking(null); }} onKeyUp={(e) => { if (e.key === "ArrowLeft" || e.key === "ArrowRight" || e.key === "Home" || e.key === "End") { void musicApi.control({ action: "seek", seconds: Number(e.currentTarget.value) }).catch((error) => onError(musicError(error))); setSeeking(null); } }} />
        <span>{musicTime(state.duration)}</span>
      </div>);
}
