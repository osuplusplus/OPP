import { visiblePoll } from "../../shared/lib/visiblePoll";
import { useEffect, useState } from "react";
import * as Switch from "@radix-ui/react-switch";
import { useQueryClient } from "@tanstack/react-query";
import { Copy, ExternalLink, FileDown, FolderOpen, Radio, RefreshCw, Save, Square, Waves } from "lucide-react";
import { ErrorPanel } from "../../shared/components/ErrorPanel";
import { PageHeader } from "../../shared/components/PageHeader";
import { Badge, Button, Card, DataLine, EmptyState, InfoTip, SectionTitle } from "../../shared/components/ui";
import { desktopApi, useCapabilities } from "../../shared/lib/tauri";
import type { ObsStatus, TosuLiveSnapshot, TosuStatus } from "../../shared/types/osu";
import { settingsQueryKey, useSettings } from "../settings/api";

const TOSU_RELEASES = "https://github.com/tosuapp/tosu/releases/latest";
const LYRICS_RELEASES = "https://github.com/HollisMeynell/tosu-lyrics/releases/latest";

function Toggle({ label, description, checked, onChange }: { label: string; description: string; checked: boolean; onChange: (value: boolean) => void }) {
  return <div className="flex items-center justify-between gap-4 rounded-lg border border-white/[0.1] bg-white/[0.035] p-4"><div className="flex items-center gap-2"><p className="font-semibold text-slate-100">{label}</p><InfoTip text={description} /></div><Switch.Root checked={checked} className="relative h-6 w-11 shrink-0 rounded-full bg-slate-500 data-[state=checked]:bg-[var(--theme-primary)]" onCheckedChange={onChange}><Switch.Thumb className="block size-5 translate-x-0.5 rounded-full bg-white transition-transform data-[state=checked]:translate-x-5" /></Switch.Root></div>;
}

function LivePanel({ live }: { live: TosuLiveSnapshot | null }) {
  if (!live) return <EmptyState icon={<Waves className="size-5" />} title="等待 tosu 实时数据" description="启动 tosu 并打开 osu! 后，这里会显示当前谱面和游戏状态。" />;
  const title = [live.artist, live.title].filter(Boolean).join(" - ") || "未读取到当前谱面";
  return <Card className="p-6"><div className="flex items-start justify-between"><div><Badge tone="pink"><Radio className="size-3.5" />{live.state ?? "已连接"}</Badge><h2 className="mt-4 text-xl font-semibold text-white">{title}</h2><p className="mt-1 text-sm text-slate-300">{[live.difficulty, live.mode, live.mods].filter(Boolean).join(" · ") || "等待游戏状态"}</p></div><p className="font-mono text-lg text-[var(--theme-primary-light)]">{live.accuracy?.toFixed(2) ?? "-"}%</p></div><div className="mt-5 grid gap-x-6 sm:grid-cols-2"><DataLine label="分数" value={live.score?.toLocaleString() ?? "-"} /><DataLine label="连击" value={`${live.combo ?? "-"} / ${live.max_combo ?? "-"}`} /><DataLine label="PP" value={live.pp_current?.toFixed(2) ?? "-"} /><DataLine label="Miss" value={live.misses ?? "-"} /></div></Card>;
}

export function TosuPage() {
  const [status, setStatus] = useState<TosuStatus | null>(null);
  const [obs, setObs] = useState<ObsStatus | null>(null);
  const [scenes, setScenes] = useState<string[]>([]);
  const [obsUrl, setObsUrl] = useState("ws://127.0.0.1:4455");
  const [obsPassword, setObsPassword] = useState("");
  const [selectedScene, setSelectedScene] = useState("");
  const [live, setLive] = useState<TosuLiveSnapshot | null>(null);
  const [busy, setBusy] = useState<"tosu" | "lyrics" | "obs" | "refresh" | null>(null);
  const [refreshMessage, setRefreshMessage] = useState<string | null>(null);
  const [error, setError] = useState<unknown>(null);
  const settings = useSettings();
  const isLinux = useCapabilities().data?.os === "linux";
  const queryClient = useQueryClient();
  const refresh = async () => { try { const [nextTosu, nextObs] = await Promise.all([desktopApi.getTosuStatus(), desktopApi.getObsStatus()]); setStatus(nextTosu); setObs(nextObs); setObsUrl(nextObs.websocket_url); setSelectedScene(nextObs.selected_scene ?? ""); setError(null); } catch (value) { setError(value); } };
  const saveSettings = async (patch: Record<string, boolean>) => { if (!settings.data) return; const saved = await desktopApi.updateSettings({ ...settings.data, ...patch }); queryClient.setQueryData(settingsQueryKey, saved); };
  const loadScenes = async () => { setBusy("obs"); try { setScenes(await desktopApi.getObsScenes()); } catch (value) { setError(value); } finally { setBusy(null); } };
  useEffect(() => { let disposed = false; let off: (() => void) | undefined; void desktopApi.onTosuLiveData((value) => { if (!disposed && document.visibilityState !== "hidden") setLive(value); }).then((unlisten) => { if (disposed) unlisten(); else off = unlisten; }); return () => { disposed = true; off?.(); }; }, []);
  const chooseTosu = async () => { const path = await desktopApi.chooseTosuExecutable(status?.executable_path); if (path) setStatus(await desktopApi.setTosuExecutable(path)); };
  const chooseLyrics = async () => { const path = await desktopApi.chooseTosuLyricsExecutable(status?.lyrics.executable_path); if (path) setStatus(await desktopApi.setTosuLyricsExecutable(path)); };
  const startStop = async () => { if (canStopTosu) { setBusy("tosu"); try { await desktopApi.stopTosu(); await refresh(); } catch (value) { setError(value); } finally { setBusy(null); } } else window.dispatchEvent(new Event("opp:request-tosu-launch")); };
  const saveObs = async () => { setBusy("obs"); try { setObs(await desktopApi.saveObsConnection(obsUrl, obsPassword || null, selectedScene || null)); setObsPassword(""); await loadScenes(); } catch (value) { setError(value); } finally { setBusy(null); } };
  const refreshSources = async () => { setBusy("refresh"); try { setRefreshMessage((await desktopApi.refreshSelectedObsScene()).message); } catch (value) { setError(value); } finally { setBusy(null); } };
  const lyrics = status?.lyrics;
  // Windows 上只能停止 OPP 自己启动的 tosu；Linux 上外部（提权）tosu 也能经 pkexec 终止。
  const canStopTosu = Boolean(status?.owned_by_opp || (isLinux && status?.running));
  // pkexec 授权需要用户输密码，轮询 tosu/OBS 状态直至就绪（不触碰正在编辑的输入框）。
  useEffect(() => { let disposed = false; let initialized = false; const stop = visiblePoll(async () => {
    const [tosu, obs] = await Promise.allSettled([desktopApi.getTosuStatus(), desktopApi.getObsStatus()]);
    if (disposed) return;
    if (tosu.status === "fulfilled") setStatus(tosu.value);
    if (obs.status === "fulfilled") {
      setObs(obs.value);
      if (!initialized) { setObsUrl(obs.value.websocket_url); setSelectedScene(obs.value.selected_scene ?? ""); initialized = true; }
    } else if (!initialized) setError(obs.reason);
  }, 2000); return () => { disposed = true; stop(); }; }, []);
  return <><PageHeader eyebrow="Live integration" title="tosu 直播集成" description="管理 tosu 服务、直播自动化和 OBS 场景刷新。" />{error ? <div className="mb-5"><ErrorPanel error={error} onRetry={() => void refresh()} /></div> : null}<div className="grid gap-5 xl:grid-cols-[1.15fr_.85fr]"><div className="space-y-5"><Card className="p-6"><div className="flex items-start justify-between"><SectionTitle title="tosu 服务" description="启动成功且 API 就绪后，会自动刷新已选择的 OBS 场景。" /><Badge tone={status?.running ? "success" : "neutral"}>{status?.running ? "运行中" : "未运行"}</Badge></div><p className="mt-4 truncate font-mono text-xs text-slate-300">{status?.executable_path ?? (isLinux ? "未检测到 PATH 中的 tosu" : "尚未选择 tosu.exe")}</p>{isLinux ? <p className="mt-2 text-xs leading-5 text-slate-400">Linux 下优先使用 PATH 中的 tosu，启动时经 pkexec 图形授权提权运行。</p> : null}<div className="mt-5 flex flex-wrap gap-3"><Button onClick={() => void desktopApi.openExternal(TOSU_RELEASES)} size="sm"><FileDown className="size-4" />官方下载页</Button><Button onClick={() => void chooseTosu()} size="sm"><FolderOpen className="size-4" />{isLinux ? "手动选择 tosu" : "选择 tosu.exe"}</Button><Button disabled={!status?.installed} loading={busy === "tosu"} onClick={() => void startStop()} variant={canStopTosu ? "danger" : "primary"}>{canStopTosu ? <Square className="size-4" /> : <Radio className="size-4" />}{canStopTosu ? "终止 tosu" : "启动 tosu"}</Button></div></Card><LivePanel live={live} /><Card className="p-6"><SectionTitle title="直播自动化" description="控制 tosu 在游戏或 OBS 启动时的行为。" /><div className="mt-5 space-y-3"><Toggle checked={settings.data?.launch_tosu_on_game_detect ?? false} description="检测到 osu! 运行后自动在后台启动 tosu。" label="检测到 osu! 后启动 tosu" onChange={(value) => void saveSettings({ launch_tosu_on_game_detect: value })} /><Toggle checked={settings.data?.launch_tosu_on_obs_detect ?? false} description="检测到 OBS 启动后自动启动已配置的 tosu。" label="检测到 OBS 后启动 tosu" onChange={(value) => void saveSettings({ launch_tosu_on_obs_detect: value })} /><Toggle checked={settings.data?.suppress_tosu_launch_prompt ?? false} description="关闭后，每次自动启动前都会显示确认提示。" label="不再显示 tosu 启动提示" onChange={(value) => void saveSettings({ suppress_tosu_launch_prompt: value })} /></div></Card></div><div className="space-y-5"><Card className="p-6"><div className="flex items-start justify-between"><SectionTitle title="OBS WebSocket" description="默认连接本机 ws://127.0.0.1:4455；密码安全保存到 Windows 凭据管理器。" /><Badge tone={obs?.connected ? "success" : "warning"}>{obs?.connected ? "已连接" : obs?.running ? "未连接" : "OBS 未运行"}</Badge></div><div className="mt-4 space-y-3"><label className="block text-sm text-slate-300">地址<input className="mt-1 w-full rounded-lg border border-white/10 bg-black/20 px-3 py-2 text-sm text-white" onChange={(event) => setObsUrl(event.target.value)} value={obsUrl} /></label><label className="block text-sm text-slate-300">密码<input className="mt-1 w-full rounded-lg border border-white/10 bg-black/20 px-3 py-2 text-sm text-white" onChange={(event) => setObsPassword(event.target.value)} placeholder="留空保留已保存密码" type="password" value={obsPassword} /></label><label className="block text-sm text-slate-300">目标场景<select className="mt-1 w-full rounded-lg border border-white/10 bg-black/20 px-3 py-2 text-sm text-white" onChange={(event) => setSelectedScene(event.target.value)} value={selectedScene}><option value="">选择场景</option>{scenes.map((scene) => <option key={scene} value={scene}>{scene}</option>)}</select></label></div><div className="mt-5 flex flex-wrap gap-2"><Button loading={busy === "obs"} onClick={() => void saveObs()} size="sm"><Save className="size-4" />保存并测试</Button><Button disabled={busy !== null} onClick={() => void loadScenes()} size="sm" variant="secondary"><RefreshCw className="size-4" />读取场景</Button><Button disabled={!selectedScene || busy !== null} loading={busy === "refresh"} onClick={() => void refreshSources()} size="sm" variant="secondary"><RefreshCw className="size-4" />刷新浏览器源</Button></div>{obs?.last_error ? <p className="mt-3 text-xs text-amber-200">{obs.last_error}</p> : null}{refreshMessage ? <p className="mt-3 text-xs text-emerald-200">{refreshMessage}</p> : null}</Card><Card className="p-6"><div className="flex items-start justify-between"><SectionTitle title="OBS 实时歌词" description="将下方地址添加为 OBS 浏览器源。" /><Badge tone={lyrics?.running ? "success" : "neutral"}>{lyrics?.running ? "运行中" : "未运行"}</Badge></div><DataLine label="浏览器源地址" value={<span className="inline-flex items-center gap-2 font-mono text-xs text-slate-100">{lyrics?.proxy_url ?? "http://127.0.0.1:41280/lyrics/"}<button aria-label="复制歌词地址" className="text-[var(--theme-primary)]" onClick={() => void navigator.clipboard.writeText(lyrics?.proxy_url ?? "http://127.0.0.1:41280/lyrics/")} type="button"><Copy className="size-3.5" /></button></span>} /><p className="mt-3 truncate font-mono text-xs text-slate-300">{lyrics?.executable_path ?? (isLinux ? "未检测到 PATH 中的 tosu-proxy" : "尚未选择 tosu-proxy.exe")}</p><div className="mt-5 flex flex-wrap gap-3"><Button onClick={() => void desktopApi.openExternal(LYRICS_RELEASES)} size="sm"><FileDown className="size-4" />歌词代理发布页</Button><Button loading={busy === "lyrics"} onClick={() => { setBusy("lyrics"); void chooseLyrics().finally(() => setBusy(null)); }} size="sm"><FolderOpen className="size-4" />{isLinux ? "手动选择 tosu 代理" : "选择 tosu-proxy.exe"}</Button><Button disabled={!lyrics?.running} onClick={() => void desktopApi.openExternal(lyrics?.proxy_url ?? "http://127.0.0.1:41280/lyrics/")} size="sm"><ExternalLink className="size-4" />打开歌词页面</Button></div></Card></div></div></>;
}
