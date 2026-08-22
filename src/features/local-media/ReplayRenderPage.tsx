import { useCallback, useEffect, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { ChevronDown, Cloud, Copy, ExternalLink, Film, LoaderCircle, MonitorPlay, MonitorUp, Search, SlidersHorizontal, Sparkles } from "lucide-react";
import { useMode } from "../../app/ModeContext";
import { ErrorPanel } from "../../shared/components/ErrorPanel";
import { PageHeader } from "../../shared/components/PageHeader";
import { Badge, Button, Card, EmptyState, SectionTitle } from "../../shared/components/ui";
import { desktopApi } from "../../shared/lib/tauri";
import type { GameMediaItem, ReplayMapInfo, ReplayRenderOptions, ReplayRenderProgress } from "../../shared/types/osu";
import { DanserRenderPanel } from "./DanserRenderPanel";
import { LivePreviewPanel } from "./LivePreviewPanel";

const defaults: ReplayRenderOptions = {
  resolution: "1280x720", global_volume: 50, music_volume: 50, hitsound_volume: 50,
  show_hit_error_meter: true, show_unstable_rate: true, show_score: true, show_hp_bar: true, show_combo_counter: true, show_pp_counter: true, show_scoreboard: true, show_borders: true, show_mods: true, show_result_screen: true, show_hit_counter: true, show_key_overlay: true, show_avatars_on_scoreboard: false, show_aim_error_meter: false, show_strain_graph: false, show_slider_breaks: false,
  use_skin_cursor: true, use_skin_colors: false, use_skin_hitsounds: true, use_beatmap_colors: true, cursor_rainbow: false, cursor_trail: true, cursor_trail_glow: false, cursor_ripples: false, cursor_size: 1,
  draw_follow_points: true, draw_combo_numbers: true, slider_snaking_in: true, slider_snaking_out: true, slider_merge: false, objects_rainbow: false, flash_objects: false, use_slider_hitcircle_color: false, beat_scaling: false,
  seizure_warning: false, load_storyboard: false, load_video: false, intro_bg_dim: 0, ingame_bg_dim: 80, break_bg_dim: 30, bg_parallax: false, show_danser_logo: true, skip_intro: true, play_nightcore_samples: true, ignore_fail: false,
};

const groups: Array<{ title: string; fields: Array<[keyof ReplayRenderOptions, string]> }> = [
  { title: "信息与叠加层", fields: [["show_score", "分数"], ["show_hp_bar", "HP 条"], ["show_combo_counter", "连击数"], ["show_pp_counter", "PP 计数"], ["show_scoreboard", "计分板"], ["show_mods", "Mod 图标"], ["show_hit_error_meter", "打击误差"], ["show_unstable_rate", "UR"], ["show_hit_counter", "判定计数"], ["show_key_overlay", "按键覆盖"], ["show_borders", "游戏区边框"], ["show_result_screen", "结算画面"]] },
  { title: "物件与光标", fields: [["use_skin_cursor", "皮肤光标"], ["cursor_trail", "光标拖尾"], ["cursor_trail_glow", "拖尾发光"], ["cursor_rainbow", "彩虹光标"], ["cursor_ripples", "点击波纹"], ["use_skin_colors", "皮肤颜色"], ["use_beatmap_colors", "谱面颜色"], ["draw_follow_points", "跟随点"], ["draw_combo_numbers", "物件连击数"], ["slider_snaking_in", "滑条进入"], ["slider_snaking_out", "滑条退出"], ["slider_merge", "滑条合并"], ["objects_rainbow", "彩虹物件"], ["flash_objects", "物件随节拍闪烁"], ["beat_scaling", "物件随节拍缩放"]] },
  { title: "背景与播放", fields: [["load_storyboard", "加载 Storyboard"], ["load_video", "加载背景视频"], ["bg_parallax", "背景视差"], ["show_danser_logo", "Danser 标志"], ["skip_intro", "跳过开场"], ["play_nightcore_samples", "Nightcore 音效"], ["ignore_fail", "忽略失败"], ["seizure_warning", "光敏警告"], ["show_avatars_on_scoreboard", "计分板头像"], ["show_aim_error_meter", "Aim Error"], ["show_strain_graph", "压力图"], ["show_slider_breaks", "滑条断裂"]] },
];

function labelForReplay(item: GameMediaItem) { return item.path.split(/[\\/]/).pop() ?? item.path; }
function OrdrRenderPanel() {
  const { client } = useMode();
  const [searchParams] = useSearchParams();
  const [replays, setReplays] = useState<GameMediaItem[]>([]);
  const [replaySearch, setReplaySearch] = useState("");
  const [replayPath, setReplayPath] = useState("");
  const [replayInfo, setReplayInfo] = useState<ReplayMapInfo | null>(null);
  const [username, setUsername] = useState("OPP");
  const [skinKind, setSkinKind] = useState<"official" | "custom">("official");
  const [skin, setSkin] = useState("default");
  const [verificationKey, setVerificationKey] = useState("");
  const [developerMode, setDeveloperMode] = useState<"success" | "api_failure" | "websocket_failure" | "">("");
  const [options, setOptions] = useState<ReplayRenderOptions>(defaults);
  const [advanced, setAdvanced] = useState(false);
  const [busy, setBusy] = useState(false);
  const [loadingReplays, setLoadingReplays] = useState(true);
  const [inspectingReplay, setInspectingReplay] = useState(false);
  const [progress, setProgress] = useState<ReplayRenderProgress | null>(null);
  const [error, setError] = useState<unknown>(null);

  const inspect = useCallback(async (path: string) => { setReplayPath(path); setReplayInfo(null); setInspectingReplay(true); try { setReplayInfo(await desktopApi.inspectGameReplay(client, path)); } catch (value) { setError(value); } finally { setInspectingReplay(false); } }, [client]);

  useEffect(() => {
    let unlisten: () => void = () => undefined;
    desktopApi.onReplayRenderProgress(setProgress).then((dispose) => { unlisten = dispose; });
    return () => unlisten();
  }, []);

  useEffect(() => {
    let active = true;
    Promise.resolve().then(() => {
      if (!active) return [];
      setLoadingReplays(true);
      return desktopApi.listGameMedia(client);
    }).then((media) => {
      if (!active) return;
      const items = media.filter((item) => item.kind === "replay");
      const requested = searchParams.get("replay");
      const selected = requested && items.some((item) => item.path === requested) ? requested : items[0]?.path ?? "";
      setReplays(items); setReplayPath(selected); setProgress(null); setError(null);
      if (selected) void inspect(selected);
    }).catch((value) => { if (active) setError(value); }).finally(() => { if (active) setLoadingReplays(false); });
    return () => { active = false; };
  }, [client, inspect, searchParams]);
  const update = <K extends keyof ReplayRenderOptions>(key: K, value: ReplayRenderOptions[K]) => setOptions((current) => ({ ...current, [key]: value }));
  const filteredReplays = replays.filter((item) => {
    const query = replaySearch.trim().toLocaleLowerCase();
    return !query || `${labelForReplay(item)} ${item.path}`.toLocaleLowerCase().includes(query);
  });
  const replaySuggestions = replays.map(labelForReplay);
  const submit = async () => {
    if (!replayPath || !replayInfo?.submitted) return;
    setBusy(true); setError(null); setProgress(null);
    try {
      const job = await desktopApi.submitReplayRender({ client, replay_path: replayPath, username, options, skin_kind: skinKind, skin, verification_key: verificationKey.trim() || null, developer_mode: developerMode || null });
      setProgress({ render_id: job.render_id, status: job.status, description: job.description, video_url: null });
    } catch (value) { setError(value); } finally { setBusy(false); }
  };
  const copy = () => progress?.video_url && void navigator.clipboard.writeText(progress.video_url);

  return <div className="pb-8">
    {error ? <div className="mb-5"><ErrorPanel error={error} onRetry={() => void submit()} /></div> : null}
    <Card className="mb-5 border-amber-300/15 bg-amber-300/[0.045] p-4 text-sm text-amber-100">o!rdr 只接收回放文件并从 osu! 下载谱面。本页仅显示本地库中<strong>已提交的 osu!standard 谱面</strong>；本地未提交谱面、其他模式或没有输入数据的回放无法渲染。</Card>
    <div className="grid gap-5 xl:grid-cols-[minmax(0,1fr)_360px]">
      <div className="space-y-5">
        <Card className="p-5"><SectionTitle title="素材" description="回放会从当前客户端的 Replays 目录安全读取；谱面用于提交前的本地兼容性校验。" />
          <label className="mt-5 block text-base font-medium text-slate-200">本地回放
            <span className="relative mt-2 block"><Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-slate-500" /><input list="replay-search-suggestions" className="w-full rounded-xl border border-white/10 bg-black/20 py-3 pl-10 pr-3 text-base text-white" value={replaySearch} onChange={(event) => setReplaySearch(event.target.value)} placeholder="搜索文件名或路径" /><datalist id="replay-search-suggestions">{replaySuggestions.map((suggestion) => <option key={suggestion} value={suggestion} />)}</datalist></span>
            <select className="mt-2 w-full rounded-xl border border-white/10 bg-black/20 p-3 text-base text-white" disabled={loadingReplays} value={replayPath} onChange={(event) => void inspect(event.target.value)}><option value="">{loadingReplays ? "正在扫描本地回放…" : "选择回放"}</option>{filteredReplays.map((item) => <option key={item.path} value={item.path}>{labelForReplay(item)}</option>)}</select>
          </label>
          {inspectingReplay ? <div className="mt-4 flex items-center gap-2 rounded-xl border border-white/10 p-4 text-sm text-slate-300"><LoaderCircle className="size-4 animate-spin" />正在校验回放…</div> : replayInfo ? <div className={`mt-4 rounded-xl border p-4 text-sm ${replayInfo.submitted ? "border-success-300/15 bg-success-300/[0.05] text-success-100" : "border-amber-300/15 bg-amber-300/[0.05] text-amber-100"}`}>{replayInfo.submitted ? `已匹配 Beatmap ID ${replayInfo.beatmap_id} · ${replayInfo.beatmap_title ?? "已提交谱面"}` : "未在本地索引中找到对应谱面，或该谱面尚未提交。请先扫描本地谱面。"}</div> : null}
        </Card>
        <Card className="p-5"><SectionTitle title="输出与音频" description="分辨率由 o!rdr 的当前权限与服务器能力决定。" />
          <div className="mt-5 grid gap-4 md:grid-cols-4"><label className="text-xs text-slate-400">分辨率<select className="mt-2 w-full rounded-xl border border-white/10 bg-black/20 p-3 text-sm text-white" value={options.resolution} onChange={(event) => update("resolution", event.target.value as ReplayRenderOptions["resolution"])}>{["720x480", "960x540", "1280x720", "1920x1080"].map((value) => <option key={value}>{value}</option>)}</select></label>{([['global_volume','总音量'], ['music_volume','音乐'], ['hitsound_volume','打击音']] as const).map(([key, label]) => <label className="text-xs text-slate-400" key={key}>{label} {options[key]}%<input className="mt-3 w-full accent-pink-400" type="range" min="0" max="100" value={options[key]} onChange={(event) => update(key, Number(event.target.value))} /></label>)}</div>
        </Card>
        <Card className="p-5"><button aria-expanded={advanced} type="button" className="flex w-full items-center justify-between text-left" onClick={() => setAdvanced((value) => !value)}><span><span className="flex items-center gap-2 text-base font-semibold text-white"><SlidersHorizontal className="size-4 text-cyan-200" />高级选项</span><span className="mt-1 block text-xs text-slate-500">覆盖层、物件、光标、背景、Storyboard 与播放行为。</span></span><ChevronDown className={`size-5 text-slate-500 transition ${advanced ? "rotate-180" : ""}`} /></button>
          {advanced ? <div className="mt-5 space-y-5">{groups.map((group) => <div key={group.title}><h3 className="mb-3 text-xs font-semibold uppercase tracking-wider text-slate-500">{group.title}</h3><div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">{group.fields.map(([key, label]) => <label className="flex cursor-pointer items-center gap-2 rounded-lg border border-white/[0.06] bg-black/15 px-3 py-2 text-xs text-slate-300" key={key}><input className="accent-pink-400" type="checkbox" checked={options[key] as boolean} onChange={(event) => update(key, event.target.checked as ReplayRenderOptions[typeof key])} />{label}</label>)}</div></div>)}<div className="grid gap-4 md:grid-cols-4"><label className="text-xs text-slate-400">光标大小<input className="mt-2 w-full rounded-xl border border-white/10 bg-black/20 p-3 text-sm text-white" type="number" min="0.5" max="2" step="0.1" value={options.cursor_size} onChange={(event) => update("cursor_size", Number(event.target.value))} /></label>{([['intro_bg_dim','开场暗度'], ['ingame_bg_dim','游戏暗度'], ['break_bg_dim','休息暗度']] as const).map(([key,label]) => <label className="text-xs text-slate-400" key={key}>{label} {options[key]}%<input className="mt-3 w-full accent-pink-400" type="range" min="0" max="100" value={options[key]} onChange={(event) => update(key, Number(event.target.value))} /></label>)}</div></div> : null}
        </Card>
      </div>
      <div className="space-y-5"><Card className="p-5"><SectionTitle title="提交身份与皮肤" description="未配置 Key 时，o!rdr 对公开请求通常限制为 5 分钟一次。" /><div className="mt-5 space-y-4"><label className="block text-sm text-slate-300">视频署名<input className="mt-2 w-full rounded-xl border border-white/10 bg-black/20 p-3 text-base text-white" value={username} maxLength={32} onChange={(event) => setUsername(event.target.value)} /></label><label className="block text-sm text-slate-300">皮肤类型<select className="mt-2 w-full rounded-xl border border-white/10 bg-black/20 p-3 text-base text-white" value={skinKind} onChange={(event) => setSkinKind(event.target.value as typeof skinKind)}><option value="official">o!rdr 官方皮肤名称</option><option value="custom">自定义皮肤 ID</option></select></label><label className="block text-sm text-slate-300">{skinKind === "custom" ? "自定义皮肤 ID" : "官方皮肤名称"}<input className="mt-2 w-full rounded-xl border border-white/10 bg-black/20 p-3 text-base text-white" value={skin} onChange={(event) => setSkin(event.target.value)} /></label><label className="block text-sm text-slate-300">验证 Key（可选，不会保存）<input className="mt-2 w-full rounded-xl border border-white/10 bg-black/20 p-3 font-mono text-base text-white" type="password" value={verificationKey} onChange={(event) => setVerificationKey(event.target.value)} /></label><label className="block text-sm text-slate-300">开发者模拟模式<select className="mt-2 w-full rounded-xl border border-white/10 bg-black/20 p-3 text-base text-white" value={developerMode} onChange={(event) => setDeveloperMode(event.target.value as typeof developerMode)}><option value="">关闭（真实提交）</option><option value="success">模拟成功</option><option value="api_failure">模拟 API 失败</option><option value="websocket_failure">模拟 WebSocket 失败</option></select></label></div><Button className="mt-5 w-full" variant="primary" loading={busy} disabled={!replayPath || !replayInfo?.submitted || !username.trim()} onClick={() => void submit()}><Film className="size-4" />提交视频生成</Button></Card>
        <Card className="p-5">{progress ? <><div className="flex items-center justify-between"><SectionTitle title={`任务 #${progress.render_id}`} description={progress.description} /><Badge tone={progress.status === "completed" ? "success" : progress.status === "failed" ? "warning" : "pink"}>{progress.status}</Badge></div>{progress.status !== "completed" && progress.status !== "failed" ? <div className="mt-5 flex items-center gap-2 text-base text-pink-100"><LoaderCircle className="size-4 animate-spin" />等待 o!rdr 更新</div> : null}{progress.video_url ? <div className="mt-5 flex flex-wrap gap-2"><Button size="sm" onClick={() => void desktopApi.openExternal(progress.video_url!)}><ExternalLink className="size-4" />打开视频</Button><Button size="sm" onClick={copy}><Copy className="size-4" />复制链接</Button><Button size="sm" onClick={() => void desktopApi.exportReplayVideo(progress.video_url!, `opp-${progress.render_id}.mp4`)}><Film className="size-4" />导出到本地</Button></div> : null}</> : <EmptyState icon={<Sparkles className="size-5" />} title="等待提交" description="提交后，o!rdr 的进度和最终视频链接会显示在这里。" />}</Card>
      </div>
    </div>
  </div>;
}

export function ReplayRenderPage() {
  const [provider, setProvider] = useState<"live" | "danser" | "ordr">(() => {
    const stored = window.localStorage.getItem("opp:replay-render-provider");
    return stored === "danser" || stored === "ordr" ? stored : "live";
  });
  const choose = (value: "danser" | "ordr" | "live") => { setProvider(value); window.localStorage.setItem("opp:replay-render-provider", value); };
  return <div className="pb-8">
    <PageHeader eyebrow="Replay studio" title="回放渲染" description="在本机使用 Danser 快速导出，或将回放提交到 o!rdr 在线渲染。" actions={<Badge tone={provider === "danser" ? "cyan" : provider === "ordr" ? "pink" : "success"}>{provider === "danser" ? "本地渲染" : provider === "ordr" ? "在线渲染" : "实时预览"}</Badge>} />
    <div className="mb-5 grid max-w-xl grid-cols-3 rounded-2xl border border-white/[0.08] bg-black/20 p-1.5" role="tablist" aria-label="渲染方式">
      <button aria-selected={provider === "live"} className={`flex items-center justify-center gap-2 rounded-xl px-4 py-3 text-sm font-semibold transition ${provider === "live" ? "selected-mask text-success-200" : "text-slate-500 hover:text-slate-200"}`} onClick={() => choose("live")} role="tab" type="button"><MonitorPlay className="size-4" />实时预览</button>
      <button aria-selected={provider === "danser"} className={`flex items-center justify-center gap-2 rounded-xl px-4 py-3 text-sm font-semibold transition ${provider === "danser" ? "selected-mask text-[var(--theme-primary-light)]" : "text-slate-500 hover:text-slate-200"}`} onClick={() => choose("danser")} role="tab" type="button"><MonitorUp className="size-4" />本地 Danser</button>
      <button aria-selected={provider === "ordr"} className={`flex items-center justify-center gap-2 rounded-xl px-4 py-3 text-sm font-semibold transition ${provider === "ordr" ? "selected-mask text-pink-200" : "text-slate-500 hover:text-slate-200"}`} onClick={() => choose("ordr")} role="tab" type="button"><Cloud className="size-4" />在线 o!rdr</button>
    </div>
    <div className={provider === "danser" ? "block" : "hidden"} aria-hidden={provider !== "danser"}><DanserRenderPanel /></div>
    <div className={provider === "ordr" ? "block" : "hidden"} aria-hidden={provider !== "ordr"}><OrdrRenderPanel /></div>
    <div className={provider === "live" ? "block" : "hidden"} aria-hidden={provider !== "live"}><LivePreviewPanel /></div>
  </div>;
}
