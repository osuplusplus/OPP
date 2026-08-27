import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ChevronDown, CircleStop, FileVideo, FolderOpen, Gauge, LoaderCircle, Play, RefreshCw, Settings2, Terminal } from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import { useSearchParams } from "react-router-dom";
import { useMode } from "../../app/ModeContext";
import { ErrorPanel } from "../../shared/components/ErrorPanel";
import { Badge, Button, Card, EmptyState, SectionTitle } from "../../shared/components/ui";
import { SearchAutocomplete } from "../../shared/components/SearchAutocomplete";
import { desktopApi, useCapabilities } from "../../shared/lib/tauri";
import type { DanserRenderJob, DanserRenderPreferences, DanserStatus, GameMediaItem, ReplayMapInfo } from "../../shared/types/osu";
import { settingsQueryKey, useSettings } from "../settings/api";

const defaultPreferences: DanserRenderPreferences = {
  settings_profile: "default", skin: "", skip: true, quickstart: false,
  start: null, end: null, speed: 1, pitch: 1, offset: 0, mods: "", mods2: "",
  cs: null, ar: null, od: null, hp: null, no_db_check: true,
  no_update_check: true, debug: false, settings_patch: "",
  frame_width: 1920, frame_height: 1080, fps: 60, encoder: "libx264",
  quality: 14, motion_blur: false, motion_blur_oversample: 16,
};

const recordingResolutions = [
  [1280, 720, "720p"],
  [1920, 1080, "1080p"],
  [2560, 1440, "1440p"],
  [3840, 2160, "4K"],
] as const;

function fileName(path: string) { return path.split(/[\\/]/).pop() ?? path; }
function numberOrNull(value: string) { return value.trim() === "" ? null : Number(value); }

export function DanserRenderPanel() {
  const { client } = useMode();
  const stored = useSettings();
  const queryClient = useQueryClient();
  const [searchParams] = useSearchParams();
  const initialized = useRef(false);
  const settingsRef = useRef(stored.data);
  const [status, setStatus] = useState<DanserStatus | null>(null);
  const [replays, setReplays] = useState<GameMediaItem[]>([]);
  const [replayPath, setReplayPath] = useState("");
  const [replaySearch, setReplaySearch] = useState("");
  const [replayInfo, setReplayInfo] = useState<ReplayMapInfo | null>(null);
  const [replayDetails, setReplayDetails] = useState<Record<string, ReplayMapInfo>>({});
  const [preferences, setPreferences] = useState(defaultPreferences);
  const [jobs, setJobs] = useState<DanserRenderJob[]>([]);
  const [advanced, setAdvanced] = useState(false);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  // Lazer：皮肤在 Realm + files/ 内容寻址存储中，Danser 用不了——入队时
  // 后端会按选择的名字导出成 Stable 布局目录；这里列出可选的 lazer 皮肤。
  const [lazerSkins, setLazerSkins] = useState<string[]>([]);
  const isLinux = useCapabilities().data?.os === "linux";

  useEffect(() => {
    if (client !== "lazer") return;
    let active = true;
    desktopApi.queryLocalSkins({ client, search: "", sort: "name", direction: "asc", offset: 0, limit: 200 })
      .then((page) => { if (active) setLazerSkins(page.items.map((item) => item.name)); })
      .catch(() => { if (active) setLazerSkins([]); });
    return () => { active = false; };
  }, [client]);

  useEffect(() => { settingsRef.current = stored.data; }, [stored.data]);

  useEffect(() => {
    if (!stored.data || initialized.current) return;
    initialized.current = true;
    setPreferences({ ...defaultPreferences, ...stored.data.danser_render_preferences });
  }, [stored.data]);

  useEffect(() => {
    if (!initialized.current || !settingsRef.current) return;
    const timer = window.setTimeout(() => {
      const current = settingsRef.current;
      if (!current) return;
      void desktopApi.updateSettings({ ...current, danser_render_preferences: preferences }).then((saved) => {
        settingsRef.current = saved;
        queryClient.setQueryData(settingsQueryKey, saved);
      }).catch(() => undefined);
    }, 500);
    return () => window.clearTimeout(timer);
  }, [preferences, queryClient]);

  const refresh = useCallback(async () => {
    setLoading(true); setError(null);
    try {
      const [nextStatus, media, queue] = await Promise.all([
        desktopApi.getDanserStatus(), desktopApi.listGameMedia(client), desktopApi.getDanserRenderQueue(),
      ]);
      setStatus(nextStatus); setJobs(queue);
      const items = media.filter((item) => item.kind === "replay");
      setReplays(items);
      const requested = searchParams.get("replay");
      setReplayPath((current) => requested && items.some((item) => item.path === requested)
        ? requested
        : current && items.some((item) => item.path === current) ? current : items[0]?.path ?? "");
    } catch (value) { setError(value); } finally { setLoading(false); }
  }, [client, searchParams]);

  useEffect(() => { const timer = window.setTimeout(() => void refresh(), 0); return () => window.clearTimeout(timer); }, [refresh]);
  useEffect(() => {
    let off: () => void = () => undefined;
    void desktopApi.onDanserRenderProgress((progress) => setJobs((current) => {
      if (progress.status === "cancelled") return current.filter((job) => job.id !== progress.id);
      const exists = current.some((job) => job.id === progress.id);
      return exists ? current.map((job) => job.id === progress.id ? progress : job) : [progress, ...current];
    })).then((unlisten) => { off = unlisten; });
    return () => off();
  }, []);

  useEffect(() => {
    if (!replayPath) return;
    let active = true;
    void desktopApi.inspectGameReplay(client, replayPath).then((value) => { if (active) setReplayInfo(value); }).catch((value) => { if (active) { setReplayInfo(null); setError(value); } });
    return () => { active = false; };
  }, [client, replayPath]);

  useEffect(() => {
    let active = true;
    let cursor = 0;
    const details: Record<string, ReplayMapInfo> = {};
    const worker = async () => {
      while (active) {
        const item = replays[cursor++];
        if (!item) return;
        try { details[item.path] = await desktopApi.inspectGameReplay(client, item.path); }
        catch { /* Invalid or unindexed replays remain searchable by file name. */ }
      }
    };
    void Promise.all(Array.from({ length: Math.min(6, replays.length) }, worker)).then(() => {
      if (active) setReplayDetails(details);
    });
    return () => { active = false; };
  }, [client, replays]);

  const patchError = useMemo(() => {
    for (const [label, value, objectOnly] of [["mods2", preferences.mods2, false], ["sPatch", preferences.settings_patch, true]] as const) {
      if (!value.trim()) continue;
      try { const parsed = JSON.parse(value); if (objectOnly && (typeof parsed !== "object" || parsed === null || Array.isArray(parsed))) return `${label} 必须是 JSON 对象`; }
      catch { return `${label} 不是有效 JSON`; }
    }
    if (preferences.mods.trim() && preferences.mods2.trim()) return "mods 与 mods2 只能填写一项";
    if (preferences.start !== null && preferences.end !== null && preferences.end <= preferences.start) return "结束时间必须大于开始时间";
    return null;
  }, [preferences]);

  const savePreferences = async () => {
    if (!settingsRef.current) return;
    const saved = await desktopApi.updateSettings({ ...settingsRef.current, danser_render_preferences: preferences });
    settingsRef.current = saved;
    queryClient.setQueryData(settingsQueryKey, saved);
  };

  const enqueue = async (path: string) => {
    if (!path) return;
    setBusy(true); setError(null);
    try {
      await savePreferences();
      const created = await desktopApi.enqueueDanserRenders({ client, replay_paths: [path], preferences });
      setJobs((current) => {
        const createdIds = new Set(created.map((job) => job.id));
        return [...created.reverse(), ...current.filter((job) => !createdIds.has(job.id))];
      });
    } catch (value) { setError(value); } finally { setBusy(false); }
  };

  const removeJob = async (job: DanserRenderJob) => {
    setJobs((current) => current.filter((item) => item.id !== job.id));
    try { await desktopApi.cancelDanserRender(job.id); }
    catch (value) { setError(value); await refresh(); }
  };

  const toggleReplay = async (path: string) => {
    const active = jobs.find((job) => job.replay_path === path && (job.status === "queued" || job.status === "running"));
    if (active) await removeJob(active);
    else await enqueue(path);
  };

  const startQueue = async () => {
    setBusy(true); setError(null);
    try { await savePreferences(); await desktopApi.startDanserRenderQueue(); }
    catch (value) { setError(value); }
    finally { setBusy(false); }
  };

  const chooseExecutable = async () => {
    if (!stored.data) return;
    const selected = await desktopApi.chooseDanserExecutable(stored.data.danser_executable_path);
    if (!selected) return;
    const saved = await desktopApi.updateSettings({ ...stored.data, danser_executable_path: selected });
    queryClient.setQueryData(settingsQueryKey, saved); await refresh();
  };

  const chooseOutput = async () => {
    if (!stored.data) return;
    const selected = await desktopApi.chooseLocalDirectory(stored.data.replay_export_directory);
    if (!selected) return;
    const saved = await desktopApi.updateSettings({ ...stored.data, replay_export_directory: selected });
    queryClient.setQueryData(settingsQueryKey, saved);
  };

  const update = <K extends keyof DanserRenderPreferences>(key: K, value: DanserRenderPreferences[K]) => setPreferences((current) => ({ ...current, [key]: value }));
  const ready = Boolean(status?.available && status.ffmpeg_available && stored.data?.replay_export_directory && !patchError);
  const normalizedSearch = replaySearch.trim().toLocaleLowerCase();
  const filteredReplays = replays.filter((item) => !normalizedSearch || `${replayDetails[item.path]?.beatmap_title ?? ""} ${fileName(item.path)} ${item.path}`.toLocaleLowerCase().includes(normalizedSearch));
  const replaySuggestions = replays.flatMap((item) => [
    { value: replayDetails[item.path]?.beatmap_title ?? fileName(item.path), label: replayDetails[item.path]?.beatmap_title ?? fileName(item.path), detail: "难度" },
  ]);
  const activePaths = new Set(jobs.filter((job) => job.status === "queued" || job.status === "running").map((job) => job.replay_path));
  const waitingCount = jobs.filter((job) => job.status === "queued").length;

  return <div>
    {error ? <div className="mb-5"><ErrorPanel error={error} onRetry={() => void refresh()} /></div> : null}
    <div className="grid gap-5 xl:grid-cols-[minmax(0,1fr)_390px]">
      <div className="space-y-5">
        <Card className="p-5">
          <div className="flex items-start justify-between gap-4"><SectionTitle title="本地运行环境" description="OPP 直接调用 danser-cli.exe；程序和 FFmpeg 均不会由 OPP 下载或修改。" /><Badge tone={status?.available && status.ffmpeg_available ? "success" : "warning"}>{status?.available && status.ffmpeg_available ? "已就绪" : "需配置"}</Badge></div>
          <div className="mt-4 rounded-xl border border-amber-300/20 bg-amber-300/[0.06] px-4 py-3 text-sm leading-6 text-amber-100"><strong>版本要求：Danser 0.11.x</strong></div>
          <div className="mt-4 rounded-xl border border-white/[0.08] bg-black/15 p-4"><p className="break-all font-mono text-xs text-slate-300">{status?.executable_path ?? (isLinux ? "未在 PATH 中找到 danser" : "未找到 danser-cli.exe")}</p><p className="mt-2 text-xs text-slate-500">{status?.message ?? "正在检测本地环境…"}</p><div className="mt-3 flex gap-2"><Button onClick={() => void chooseExecutable()} size="sm"><FolderOpen className="size-4" />选择程序</Button><Button disabled={loading} onClick={() => void refresh()} size="sm"><RefreshCw className={`size-4 ${loading ? "animate-spin" : ""}`} />重新检测</Button></div></div>
        </Card>

        <Card className="p-5"><SectionTitle title="回放与输出" description="搜索回放并点击条目即可加入待渲染队列；Danser 只支持 osu!standard。" />
          <SearchAutocomplete aria-label="搜索本地回放" className="mt-5" inputClassName="opp-input w-full py-3 pl-10 pr-4" onChange={setReplaySearch} placeholder="搜索回放文件名或路径" suggestions={replaySuggestions} value={replaySearch} />
          <div className="mt-3 max-h-64 space-y-2 overflow-y-auto pr-1">{filteredReplays.length ? filteredReplays.map((item) => { const active = activePaths.has(item.path); const detail = replayDetails[item.path]; return <button className={`flex w-full items-center gap-3 rounded-xl border p-3 text-left transition ${active ? "border-cyan-300/20 bg-cyan-300/[0.06]" : "border-white/[0.07] bg-black/15 hover:border-white/[0.16]"}`} disabled={!ready || busy || (detail ? !detail.beatmap_title : false)} key={item.path} onClick={() => { setReplayPath(item.path); void toggleReplay(item.path); }} type="button"><FileVideo className="size-4 shrink-0 text-pink-300" /><span className="min-w-0 flex-1"><span className="block truncate text-sm text-slate-200">{detail?.beatmap_title ?? fileName(item.path)}</span><span className="mt-1 block truncate text-xs text-slate-500">{detail ? (detail.beatmap_title ? (active ? "点击移出队列" : `${fileName(item.path)} · 点击加入队列`) : "未匹配到本地谱面") : "正在识别对应难度…"}</span></span>{active ? <Badge tone="cyan">已加入</Badge> : null}</button>; }) : <EmptyState title="没有匹配的回放" description="换一个文件名、难度名或路径关键词试试。" />}</div>
          {replayPath ? <div className={`mt-3 rounded-xl border p-3 text-sm ${replayInfo?.beatmap_title ? "border-emerald-300/15 bg-emerald-300/[0.05] text-emerald-100" : "border-amber-300/15 bg-amber-300/[0.05] text-amber-100"}`}>{replayInfo?.beatmap_title ? `${replayInfo.beatmap_title} · ${replayInfo.username}` : "未在本地索引中匹配到谱面，暂不能交给 Danser。"}</div> : null}
          <div className="mt-4 rounded-xl border border-white/[0.08] bg-black/15 p-4"><p className="text-xs text-slate-500">统一导出目录</p><p className="mt-1 break-all text-sm text-slate-200">{stored.data?.replay_export_directory ?? "尚未设置"}</p><Button className="mt-3" onClick={() => void chooseOutput()} size="sm"><FolderOpen className="size-4" />选择目录</Button></div>
        </Card>

        <Card className="p-5"><SectionTitle title="视频质量" description="这些选项只作用于本次导出，不会修改 Danser 的配置文件。" />
          <div className="mt-5 grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
            <label className="text-xs text-slate-400">分辨率<select className="opp-input mt-2 w-full" value={`${preferences.frame_width}x${preferences.frame_height}`} onChange={(event) => { const [width, height] = event.target.value.split("x").map(Number); setPreferences((current) => ({ ...current, frame_width: width, frame_height: height })); }}>{recordingResolutions.map(([width, height, label]) => <option key={`${width}x${height}`} value={`${width}x${height}`}>{label}（{width} × {height}）</option>)}</select></label>
            <label className="text-xs text-slate-400">帧率<select className="opp-input mt-2 w-full" value={preferences.fps} onChange={(event) => update("fps", Number(event.target.value))}>{[30, 60, 120, 240].map((fps) => <option key={fps} value={fps}>{fps} FPS</option>)}</select></label>
            <label className="text-xs text-slate-400">视频编码器<select className="opp-input mt-2 w-full" value={preferences.encoder} onChange={(event) => update("encoder", event.target.value as DanserRenderPreferences["encoder"])}><option value="libx264">CPU · H.264（兼容性最好）</option><option value="h264_nvenc">NVIDIA NVENC · H.264</option><option value="h264_qsv">Intel Quick Sync · H.264</option></select></label>
            <label className="text-xs text-slate-400">{preferences.encoder === "libx264" ? "画质（CRF，越低越清晰）" : "画质（越低越清晰）"}<input className="opp-input mt-2 w-full" max="51" min="0" type="number" value={preferences.quality} onChange={(event) => update("quality", Math.max(0, Math.min(51, Number(event.target.value) || 0)))} /></label>
            {preferences.motion_blur ? <label className="text-xs text-slate-400">运动模糊采样<input className="opp-input mt-2 w-full" max="64" min="2" type="number" value={preferences.motion_blur_oversample} onChange={(event) => update("motion_blur_oversample", Math.max(2, Math.min(64, Number(event.target.value) || 2)))} /></label> : null}
          </div>
          <label className="mt-4 flex items-center gap-2 rounded-lg border border-white/[0.08] px-3 py-2 text-sm text-slate-300"><input checked={preferences.motion_blur} onChange={(event) => update("motion_blur", event.target.checked)} type="checkbox" />运动模糊（显著增加渲染时间）</label>
          {preferences.encoder !== "libx264" ? <p className="mt-3 text-xs leading-5 text-amber-200/80">硬件编码需要对应显卡和 FFmpeg 编码器支持；不可用时任务会显示 Danser 的失败原因。</p> : null}
        </Card>

        <Card className="p-5"><SectionTitle title="常用参数" description="OPP 会自动附加 replay、record、out 与 preciseprogress 参数。" />
          <div className="mt-5 grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
            <label className="text-xs text-slate-400">配置档<select className="opp-input mt-2 w-full" value={preferences.settings_profile} onChange={(event) => update("settings_profile", event.target.value)}>{(status?.profiles.length ? status.profiles : ["default"]).map((profile) => <option key={profile}>{profile}</option>)}</select></label>
            <label className="text-xs text-slate-400">皮肤覆盖{client === "lazer"
              ? <select className="opp-input mt-2 w-full" value={lazerSkins.includes(preferences.skin) || preferences.skin === "" ? preferences.skin : ""} onChange={(event) => update("skin", event.target.value)}><option value="">留空使用 Danser 默认皮肤</option>{lazerSkins.map((name) => <option key={name} value={name}>{name}</option>)}</select>
              : <input className="opp-input mt-2 w-full" value={preferences.skin} onChange={(event) => update("skin", event.target.value)} placeholder="留空使用配置档" />}</label>
            <label className="text-xs text-slate-400">音频偏移（ms）<input className="opp-input mt-2 w-full" type="number" value={preferences.offset} onChange={(event) => update("offset", Number(event.target.value))} /></label>
            <label className="text-xs text-slate-400">开始时间（秒）<input className="opp-input mt-2 w-full" min="0" step="0.1" type="number" value={preferences.start ?? ""} onChange={(event) => update("start", numberOrNull(event.target.value))} /></label>
            <label className="text-xs text-slate-400">结束时间（秒）<input className="opp-input mt-2 w-full" min="0" step="0.1" type="number" value={preferences.end ?? ""} onChange={(event) => update("end", numberOrNull(event.target.value))} /></label>
            <label className="text-xs text-slate-400">速度<input className="opp-input mt-2 w-full" min="0.1" max="4" step="0.05" type="number" value={preferences.speed} onChange={(event) => update("speed", Number(event.target.value))} /></label>
            <label className="text-xs text-slate-400">音高<input className="opp-input mt-2 w-full" min="0.1" max="4" step="0.05" type="number" value={preferences.pitch} onChange={(event) => update("pitch", Number(event.target.value))} /></label>
          </div>
          <div className="mt-4 flex flex-wrap gap-3">{([["skip", "跳过开场"], ["quickstart", "快速开始"]] as const).map(([key, label]) => <label className="flex items-center gap-2 rounded-lg border border-white/[0.08] px-3 py-2 text-sm text-slate-300" key={key}><input checked={preferences[key]} onChange={(event) => update(key, event.target.checked)} type="checkbox" />{label}</label>)}</div>
        </Card>

        <Card className="p-5"><button className="flex w-full items-center justify-between text-left" onClick={() => setAdvanced((value) => !value)} type="button"><span><span className="flex items-center gap-2 font-semibold text-white"><Settings2 className="size-4 text-cyan-200" />高级 CLI 参数</span><span className="mt-1 block text-xs text-slate-500">Mod、难度覆盖、数据库行为与配置补丁</span></span><ChevronDown className={`size-5 transition ${advanced ? "rotate-180" : ""}`} /></button>
          {advanced ? <div className="mt-5 space-y-4"><div className="grid gap-4 sm:grid-cols-2"><label className="text-xs text-slate-400">mods<input className="opp-input mt-2 w-full" value={preferences.mods} onChange={(event) => update("mods", event.target.value)} placeholder="HDHR" /></label><label className="text-xs text-slate-400">mods2 JSON<input className="opp-input mt-2 w-full" value={preferences.mods2} onChange={(event) => update("mods2", event.target.value)} placeholder='[{"acronym":"DT"}]' /></label></div><div className="grid grid-cols-2 gap-4 sm:grid-cols-4">{(["cs", "ar", "od", "hp"] as const).map((key) => <label className="text-xs uppercase text-slate-400" key={key}>{key}<input className="opp-input mt-2 w-full" type="number" step="0.1" value={preferences[key] ?? ""} onChange={(event) => update(key, numberOrNull(event.target.value))} /></label>)}</div><label className="block text-xs text-slate-400">sPatch JSON<textarea className="opp-input mt-2 min-h-28 w-full font-mono" value={preferences.settings_patch} onChange={(event) => update("settings_patch", event.target.value)} placeholder='{"Cursor":{"CursorSize":50}}' /></label><div className="flex flex-wrap gap-3">{([["no_db_check", "跳过谱面库更新"], ["no_update_check", "跳过版本检查"], ["debug", "调试输出"]] as const).map(([key, label]) => <label className="flex items-center gap-2 text-sm text-slate-300" key={key}><input checked={preferences[key]} onChange={(event) => update(key, event.target.checked)} type="checkbox" />{label}</label>)}</div>{patchError ? <p className="text-sm text-rose-200">{patchError}</p> : null}</div> : null}
        </Card>
      </div>

      <div className="space-y-5"><Card className="p-5"><SectionTitle title="开始本地渲染" description={`已有 ${waitingCount} 个任务等待开始；参数会在加入队列时保存。`} /><Button className="mt-5 w-full" disabled={!ready || waitingCount === 0} loading={busy} onClick={() => void startQueue()} variant="primary"><Play className="size-4" />开始渲染队列</Button>{!ready ? <p className="mt-3 text-xs leading-5 text-slate-500">请确认 Danser、FFmpeg 和导出目录均已就绪。</p> : null}</Card>
        <Card className="p-5"><div className="flex items-start justify-between"><SectionTitle title="本地渲染队列" description="同一时间只运行一个任务，避免抢占显卡。" /><Gauge className="size-5 text-cyan-200" /></div><div className="mt-4 space-y-3">{jobs.length ? jobs.map((job) => <div className="rounded-xl border border-white/[0.08] bg-black/15 p-3" key={job.id}><div className="flex items-start justify-between gap-3"><div className="min-w-0"><p className="truncate text-sm font-medium text-slate-200">{fileName(job.replay_path)}</p><p className="mt-1 text-xs text-slate-500">{job.description}</p></div><Badge tone={job.status === "completed" ? "success" : job.status === "failed" ? "warning" : job.status === "running" ? "cyan" : "neutral"}>{job.status}</Badge></div>{job.status === "running" ? <div className="mt-3 h-1.5 overflow-hidden rounded-full bg-white/[0.06]"><div className="h-full rounded-full bg-[var(--theme-primary)] transition-all" style={{ width: `${job.progress}%` }} /></div> : null}<div className="mt-3 flex flex-wrap gap-2">{job.status === "running" || job.status === "queued" ? <Button onClick={() => void removeJob(job)} size="sm"><CircleStop className="size-4" />取消</Button> : null}{job.output_path ? <Button onClick={() => void desktopApi.openDanserOutput(job.output_path!)} size="sm"><FolderOpen className="size-4" />定位视频</Button> : null}</div></div>) : <EmptyState icon={loading ? <LoaderCircle className="size-5 animate-spin" /> : <Terminal className="size-5" />} title={loading ? "正在读取队列" : "队列为空"} description="选择回放和参数后，将任务加入本地队列。" />}</div></Card>
      </div>
    </div>
  </div>;
}
