import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { ChevronDown, Copy, ExternalLink, Film, LoaderCircle, SlidersHorizontal, Sparkles } from "lucide-react";
import { ErrorPanel } from "../../shared/components/ErrorPanel";
import { Badge, Button, Card, EmptyState, SectionTitle } from "../../shared/components/ui";
import { desktopApi } from "../../shared/lib/tauri";
import type { ReplayRenderOptions, ReplayRenderProgress } from "../../shared/types/osu";
import { useRenderSession, useReplayWorkspace } from "./api";
import { replayBlockReason } from "./model";
import { ReplayIdentity, StudioTasks } from "./ReplayWorkspace";

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

export function OrdrRenderPanel() {
  const cache = useQueryClient();
  const { client, replayPath, replayInfo, inspectError } = useReplayWorkspace();
  const blocked = replayBlockReason(replayPath, replayInfo, "ordr", inspectError);
  const [username, setUsername] = useRenderSession("ordr-username", "OPP");
  const [skinKind, setSkinKind] = useRenderSession<"official" | "custom">("ordr-skin-kind", "official");
  const [skin, setSkin] = useRenderSession("ordr-skin", "default");
  const [verificationKey, setVerificationKey] = useState("");
  const [developerMode, setDeveloperMode] = useState<"success" | "api_failure" | "websocket_failure" | "">("");
  const [options, setOptions] = useRenderSession<ReplayRenderOptions>("ordr-options", defaults);
  const [advanced, setAdvanced] = useState(false);
  const [busy, setBusy] = useRenderSession("ordr-submitting", false);
  const [progress, setProgress] = useRenderSession<ReplayRenderProgress | null>("ordr-progress", null);
  const [error, setError] = useState<unknown>(null);
  const pending = busy || Boolean(progress && !["completed", "failed"].includes(progress.status));

  const update = <K extends keyof ReplayRenderOptions>(key: K, value: ReplayRenderOptions[K]) => setOptions((current) => ({ ...current, [key]: value }));
  const submit = async () => {
    if (blocked || pending) return;
    setBusy(true); setError(null); setProgress(null);
    try {
      const job = await desktopApi.submitReplayRender({ client, replay_path: replayPath, username, options, skin_kind: skinKind, skin, verification_key: verificationKey.trim() || null, developer_mode: developerMode || null });
      setProgress(cache.getQueryData<ReplayRenderProgress>(["replay-studio-session", `ordr-event:${job.render_id}`]) ?? { render_id: job.render_id, status: job.status, description: job.description, video_url: null });
    } catch (value) { setError(value); } finally { setBusy(false); }
  };
  const copy = () => progress?.video_url && void navigator.clipboard.writeText(progress.video_url);

  return <div className="studio-panel">
    {error ? <ErrorPanel error={error} onRetry={() => void submit()} /> : null}
    <div className="studio-grid"><ReplayIdentity provider="ordr"><p className="studio-provider-note">o!rdr 只接收回放文件，并从 osu! 下载已提交的 standard 谱面。</p></ReplayIdentity>
    <aside className="studio-settings" aria-label="在线渲染设置"><div className="studio-settings-scroll">        <Card className="p-5"><SectionTitle title="输出与音频" description="分辨率由 o!rdr 的当前权限与服务器能力决定。" />
          <div className="mt-5 grid gap-4 md:grid-cols-4"><label className="text-xs text-slate-400">分辨率<select className="mt-2 w-full rounded-xl border border-white/10 bg-black/20 p-3 text-sm text-white" value={options.resolution} onChange={(event) => update("resolution", event.target.value as ReplayRenderOptions["resolution"])}>{["720x480", "960x540", "1280x720", "1920x1080"].map((value) => <option key={value}>{value}</option>)}</select></label>{([['global_volume','总音量'], ['music_volume','音乐'], ['hitsound_volume','打击音']] as const).map(([key, label]) => <label className="text-xs text-slate-400" key={key}>{label} {options[key]}%<input className="mt-3 w-full accent-pink-400" type="range" min="0" max="100" value={options[key]} onChange={(event) => update(key, Number(event.target.value))} /></label>)}</div>
        </Card>
        <Card className="p-5"><button aria-expanded={advanced} type="button" className="flex w-full items-center justify-between text-left" onClick={() => setAdvanced((value) => !value)}><span><span className="flex items-center gap-2 text-base font-semibold text-white"><SlidersHorizontal className="size-4 text-cyan-200" />高级选项</span><span className="mt-1 block text-xs text-slate-500">覆盖层、物件、光标、背景、Storyboard 与播放行为。</span></span><ChevronDown className={`size-5 text-slate-500 transition ${advanced ? "rotate-180" : ""}`} /></button>
          {advanced ? <div className="mt-5 space-y-5">{groups.map((group) => <div key={group.title}><h3 className="mb-3 text-xs font-semibold uppercase tracking-wider text-slate-500">{group.title}</h3><div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">{group.fields.map(([key, label]) => <label className="flex cursor-pointer items-center gap-2 rounded-lg border border-white/[0.06] bg-black/15 px-3 py-2 text-xs text-slate-300" key={key}><input className="accent-pink-400" type="checkbox" checked={options[key] as boolean} onChange={(event) => update(key, event.target.checked as ReplayRenderOptions[typeof key])} />{label}</label>)}</div></div>)}<div className="grid gap-4 md:grid-cols-4"><label className="text-xs text-slate-400">光标大小<input className="mt-2 w-full rounded-xl border border-white/10 bg-black/20 p-3 text-sm text-white" type="number" min="0.5" max="2" step="0.1" value={options.cursor_size} onChange={(event) => update("cursor_size", Number(event.target.value))} /></label>{([['intro_bg_dim','开场暗度'], ['ingame_bg_dim','游戏暗度'], ['break_bg_dim','休息暗度']] as const).map(([key,label]) => <label className="text-xs text-slate-400" key={key}>{label} {options[key]}%<input className="mt-3 w-full accent-pink-400" type="range" min="0" max="100" value={options[key]} onChange={(event) => update(key, Number(event.target.value))} /></label>)}</div></div> : null}
        </Card>
<Card className="p-5"><SectionTitle title="提交身份与皮肤" description="未配置 Key 时，o!rdr 对公开请求通常限制为 5 分钟一次。" /><div className="mt-5 space-y-4"><label className="block text-sm text-slate-300">视频署名<input className="mt-2 w-full rounded-xl border border-white/10 bg-black/20 p-3 text-base text-white" value={username} maxLength={32} onChange={(event) => setUsername(event.target.value)} /></label><label className="block text-sm text-slate-300">皮肤类型<select className="mt-2 w-full rounded-xl border border-white/10 bg-black/20 p-3 text-base text-white" value={skinKind} onChange={(event) => setSkinKind(event.target.value as typeof skinKind)}><option value="official">o!rdr 官方皮肤名称</option><option value="custom">自定义皮肤 ID</option></select></label><label className="block text-sm text-slate-300">{skinKind === "custom" ? "自定义皮肤 ID" : "官方皮肤名称"}<input className="mt-2 w-full rounded-xl border border-white/10 bg-black/20 p-3 text-base text-white" value={skin} onChange={(event) => setSkin(event.target.value)} /></label><label className="block text-sm text-slate-300">验证 Key（可选，不会保存）<input className="mt-2 w-full rounded-xl border border-white/10 bg-black/20 p-3 font-mono text-base text-white" type="password" value={verificationKey} onChange={(event) => setVerificationKey(event.target.value)} /></label><details className="studio-debug"><summary>开发者调试</summary><label className="block text-sm text-slate-300">开发者模拟模式<select className="mt-2 w-full rounded-xl border border-white/10 bg-black/20 p-3 text-base text-white" value={developerMode} onChange={(event) => setDeveloperMode(event.target.value as typeof developerMode)}><option value="">关闭（真实提交）</option><option value="success">模拟成功</option><option value="api_failure">模拟 API 失败</option><option value="websocket_failure">模拟 WebSocket 失败</option></select></label></details></div></Card></div><div className="studio-action-bar"><p className="studio-disabled-reason">{blocked || (pending ? "当前任务进行中，完成后可提交下一份回放" : !username.trim() ? "请填写视频署名" : null)}</p><Button className="mt-5 w-full studio-primary-action" variant="primary" loading={busy} disabled={Boolean(blocked) || !username.trim() || pending} onClick={() => void submit()}><Film className="size-4" />提交视频生成</Button></div></aside></div>
    <StudioTasks provider="ordr" title={progress ? `任务 #${progress.render_id} · ${progress.description}` : "等待提交"}>        <Card className="p-5">{progress ? <><div className="flex items-center justify-between"><SectionTitle title={`任务 #${progress.render_id}`} description={progress.description} /><Badge tone={progress.status === "completed" ? "success" : progress.status === "failed" ? "warning" : "pink"}>{progress.status}</Badge></div>{progress.status !== "completed" && progress.status !== "failed" ? <div className="mt-5 flex items-center gap-2 text-base text-pink-100"><LoaderCircle className="size-4 animate-spin" />等待 o!rdr 更新</div> : null}{progress.video_url ? <div className="mt-5 flex flex-wrap gap-2"><Button size="sm" onClick={() => void desktopApi.openExternal(progress.video_url!)}><ExternalLink className="size-4" />打开视频</Button><Button size="sm" onClick={copy}><Copy className="size-4" />复制链接</Button><Button size="sm" onClick={() => void desktopApi.exportReplayVideo(progress.video_url!, `opp-${progress.render_id}.mp4`)}><Film className="size-4" />导出到本地</Button></div> : null}</> : <EmptyState icon={<Sparkles className="size-5" />} title="等待提交" description="提交后，o!rdr 的进度和最终视频链接会显示在这里。" />}</Card></StudioTasks>
  </div>;
}
