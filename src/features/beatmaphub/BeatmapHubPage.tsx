import { useMemo, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Copy, Download, Heart, Info, Link2, LogOut, PackageOpen, RefreshCw, Send, ShieldCheck, Star, Trash2, Users } from "lucide-react";
import { PageHeader } from "../../shared/components/PageHeader";
import { Badge, Button, Card, EmptyState } from "../../shared/components/ui";
import { desktopApi } from "../../shared/lib/tauri";
import type { BeatmapHubPack, CommandError, OnlineBeatmapset } from "../../shared/types/osu";
import { useAuthStatus } from "../auth/api";
import { collectionsQueryKey, useCollections } from "../collections/api";
import { resolveDefaultDownloadProvider } from "../online-beatmaps/downloadProvider";
import { beatmapHubAuthKey, beatmapHubProfileKey, useBeatmapHubAuth, useBeatmapHubProfile } from "./api";

function errorText(error: unknown) {
  const value = error as CommandError;
  return `${value?.message ?? String(error)}${value?.request_id ? `（请求 ID：${value.request_id}）` : ""}`;
}

async function resolveSets(ids: number[], onProgress: (done: number) => void) {
  const resolved: OnlineBeatmapset[] = [];
  for (let offset = 0; offset < ids.length; offset += 6) {
    const values = await Promise.all(ids.slice(offset, offset + 6).map(async (id) => {
      try { return await desktopApi.getOnlineBeatmapset(id); } catch { return null; }
    }));
    resolved.push(...values.filter((value): value is OnlineBeatmapset => value !== null));
    onProgress(Math.min(ids.length, offset + 6));
  }
  return resolved;
}

function IdentitySetup({ defaultName, defaultDevice }: { defaultName: string; defaultDevice: string }) {
  const queryClient = useQueryClient();
  const [mode, setMode] = useState<"create" | "link">("create");
  const [displayName, setDisplayName] = useState(defaultName);
  const [deviceName, setDeviceName] = useState(defaultDevice);
  const [token, setToken] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const submit = async () => {
    setBusy(true); setError(null);
    try {
      if (mode === "create") await desktopApi.createBeatmapHubProfile(displayName, deviceName);
      else await desktopApi.linkBeatmapHubDevice(token.trim(), deviceName);
      await queryClient.invalidateQueries({ queryKey: beatmapHubAuthKey });
    } catch (caught) { setError(errorText(caught)); } finally { setBusy(false); }
  };
  return <Card className="mx-auto max-w-2xl p-7">
    <div className="flex items-center gap-3"><span className="grid size-11 place-items-center rounded-xl bg-cyan-300/10 text-cyan-200"><ShieldCheck className="size-6" /></span><div><h2 className="text-lg font-semibold text-white">连接 BeatmapHub</h2><p className="text-sm text-slate-500">使用本设备密钥建立独立社区身份，不会改变 osu! 登录。</p></div></div>
    <div className="mt-6 flex gap-2"><Button onClick={() => setMode("create")} variant={mode === "create" ? "primary" : "secondary"}>创建新档案</Button><Button onClick={() => setMode("link")} variant={mode === "link" ? "primary" : "secondary"}>链接已有档案</Button></div>
    <div className="mt-5 grid gap-4 sm:grid-cols-2">
      {mode === "create" ? <label className="text-xs text-slate-400">显示名<input className="opp-input mt-1.5" maxLength={64} onChange={(event) => setDisplayName(event.target.value)} value={displayName} /></label> : null}
      <label className="text-xs text-slate-400">设备名<input className="opp-input mt-1.5" maxLength={64} onChange={(event) => setDeviceName(event.target.value)} value={deviceName} /></label>
      {mode === "link" ? <label className="text-xs text-slate-400 sm:col-span-2">一次性链接码<input className="opp-input mt-1.5 font-mono" onChange={(event) => setToken(event.target.value)} placeholder="粘贴旧设备生成的 43 位链接码" value={token} /></label> : null}
    </div>
    <Button className="mt-5" disabled={!deviceName.trim() || (mode === "create" ? !displayName.trim() : token.trim().length !== 43)} loading={busy} onClick={() => void submit()}>{mode === "create" ? "创建并连接" : "链接此设备"}</Button>
    {error ? <p className="mt-4 text-sm text-rose-200" role="alert">{error}</p> : null}
    <p className="mt-5 text-xs leading-5 text-slate-600">私钥仅保存在系统安全存储中。若所有已登记设备均丢失，档案无法人工找回。</p>
  </Card>;
}

export function BeatmapHubPage() {
  const queryClient = useQueryClient();
  const auth = useBeatmapHubAuth();
  const osuAuth = useAuthStatus();
  const collections = useCollections();
  const profile = useBeatmapHubProfile(Boolean(auth.data?.connected));
  const writable = useMemo(() => (collections.data?.folders ?? []).filter((folder) => !folder.read_only && folder.source !== "lazer"), [collections.data]);
  const [shareCode, setShareCode] = useState("");
  const [pack, setPack] = useState<BeatmapHubPack | null>(null);
  const [resolved, setResolved] = useState<OnlineBeatmapset[]>([]);
  const [resolveProgress, setResolveProgress] = useState(0);
  const [folderId, setFolderId] = useState("");
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [downloadMissing, setDownloadMissing] = useState(true);
  const [linkToken, setLinkToken] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refreshPack = async (id: string) => {
    const preview = await desktopApi.previewBeatmapHubPack(id);
    const value = preview.pack;
    setNotice(`本地已有 ${preview.locally_available_ids.length} 个谱面集，缺失 ${preview.missing_ids.length} 个。`);
    setPack(value); setShareCode(`BPH-${value.id}`); setResolveProgress(0);
    setTitle(value.title); setDescription(value.description);
    setResolved(await resolveSets(value.beatmapset_ids, setResolveProgress));
  };
  const run = async (key: string, action: () => Promise<void>) => {
    setBusy(key); setError(null); setNotice(null);
    try { await action(); } catch (caught) { setError(errorText(caught)); } finally { setBusy(null); }
  };
  const openPack = () => run("open", async () => { await refreshPack(shareCode); });
  const publish = () => run("publish", async () => {
    const result = await desktopApi.publishBeatmapHubPack(folderId, title, description);
    await navigator.clipboard?.writeText(`BPH-${result.id}`);
    setNotice(`发布成功：BPH-${result.id}，包含 ${result.included} 个谱面集${result.skipped ? `，跳过 ${result.skipped} 个无 ID 条目` : ""}`);
    setShareCode(result.id); await refreshPack(result.id);
  });
  const importPack = () => run("import", async () => {
    if (!pack) return;
    const imported = await desktopApi.importBeatmapHubPack(pack.id, resolved);
    await queryClient.invalidateQueries({ queryKey: collectionsQueryKey });
    let message = `已创建收藏夹，导入 ${imported.imported_sets} 个谱面集、${imported.imported_entries} 个条目`;
    if (downloadMissing) {
      await desktopApi.beginCollectionTask();
      const items = await desktopApi.getCollectionDownloadItems([imported.folder_id]);
      if (items.length) {
        const settings = await desktopApi.getSettings();
        const sources = await desktopApi.getLocalSources();
        const root = sources.find((source) => source.client === "stable")?.install_root;
        if (!root) throw new Error("请先配置 osu!stable 目录后再自动下载");
        const destination = settings.beatmap_download_directory || `${root}\\OPP Downloads`;
        const download = await desktopApi.downloadOnlineBeatmapsets({ destination, provider: resolveDefaultDownloadProvider(settings), overwrite: false, include_video: settings.include_video_in_beatmap_downloads, open_after_download: false, items });
        if (download.completed_paths?.length) await desktopApi.installCollectionDownloads([imported.folder_id], download.completed_paths);
        message += `；下载成功 ${download.completed}，失败 ${download.failed}`;
      } else message += "；本地无需补齐";
    }
    setNotice(message);
  });

  if (auth.isLoading || osuAuth.isLoading) return <div className="p-8 text-sm text-slate-500">正在读取 BeatmapHub 身份…</div>;
  if (!auth.data?.has_identity) return <div className="space-y-6"><PageHeader eyebrow="COMMUNITY" title="BeatmapHub" description="发布与导入社区曲包" /><IdentitySetup defaultName={osuAuth.data?.username ?? "Player"} defaultDevice={auth.data?.device_name ?? "Desktop PC"} /></div>;

  const selected = writable.find((folder) => folder.id === folderId);
  return <div className="space-y-6">
    <PageHeader eyebrow="COMMUNITY" title="BeatmapHub" description="通过短分享码发布、评价和导入社区曲包" />
    <Card className="border-cyan-300/15 bg-cyan-300/[0.045] p-4">
      <div className="flex items-start gap-3"><Info className="mt-0.5 size-4 shrink-0 text-cyan-200" /><div><p className="text-sm font-medium text-cyan-100">使用提示</p><p className="mt-1 text-xs leading-5 text-slate-400">分享码只包含谱面集清单，不上传本地谱面文件。导入会新建本地收藏夹；勾选“同时下载缺失谱面”时，需要先在设置中配置有效的 osu!stable 目录，下载源、保存位置及是否包含视频均沿用“设置 → 谱面下载”。</p></div></div>
    </Card>
    {!auth.data.connected ? <><Card className="flex items-center justify-between gap-4 p-5"><div><p className="font-medium text-amber-100">Hub Session 已过期</p><p className="mt-1 text-xs text-slate-500">设备私钥仍安全保留，可以直接重新验证。</p></div><Button loading={busy === "login"} onClick={() => void run("login", async () => { await desktopApi.loginBeatmapHub(); await queryClient.invalidateQueries({ queryKey: beatmapHubAuthKey }); })}><RefreshCw className="size-4" />重新连接</Button></Card><details className="rounded-xl border border-white/[0.08] bg-black/10 p-4"><summary className="cursor-pointer text-sm text-slate-400">设备已撤销或私钥不可用？重新建立身份</summary><div className="mt-4"><IdentitySetup defaultName={auth.data.display_name ?? osuAuth.data?.username ?? "Player"} defaultDevice={auth.data.device_name ?? "Desktop PC"} /></div></details></> : null}
    <div className="grid gap-5 xl:grid-cols-2">
      <Card className="p-5"><div className="flex items-center gap-2"><PackageOpen className="size-5 text-cyan-200" /><h2 className="font-semibold text-white">打开社区曲包</h2></div><p className="mt-1 text-xs text-slate-500">输入 6 位分享码，支持 BPH- 前缀。</p><div className="mt-4 flex gap-2"><input className="opp-input font-mono uppercase" onChange={(event) => setShareCode(event.target.value)} placeholder="BPH-7K3N9A" value={shareCode} /><Button disabled={!shareCode.trim()} loading={busy === "open"} onClick={() => void openPack()}>打开</Button></div></Card>
      <Card className="p-5"><div className="flex items-center gap-2"><Send className="size-5 text-violet-200" /><h2 className="font-semibold text-white">发布本地收藏夹</h2></div><div className="mt-4 grid gap-3"><select className="opp-input" onChange={(event) => { const id = event.target.value; setFolderId(id); const folder = writable.find((value) => value.id === id); if (folder) setTitle(folder.name); }} value={folderId}><option value="">选择收藏夹</option>{writable.map((folder) => <option key={folder.id} value={folder.id}>{folder.name}（{new Set(folder.entries.map((entry) => entry.beatmapset_id).filter(Boolean)).size} 个谱面集）</option>)}</select><input className="opp-input" maxLength={120} onChange={(event) => setTitle(event.target.value)} placeholder="曲包标题" value={title} /><textarea className="opp-input min-h-20 resize-y" maxLength={2000} onChange={(event) => setDescription(event.target.value)} placeholder="用途、难度或玩法说明（可选）" value={description} /><Button disabled={!selected || !title.trim()} loading={busy === "publish"} onClick={() => void publish()}>发布并复制分享码</Button></div></Card>
    </div>
    {pack ? <Card className="overflow-hidden"><div className="border-b border-white/[0.08] p-6"><div className="flex flex-wrap items-start justify-between gap-4"><div><div className="flex items-center gap-2"><Badge tone="success">BPH-{pack.id}</Badge><span className="text-xs text-slate-500">{pack.owner.display_name}</span></div><h2 className="mt-3 text-xl font-semibold text-white">{pack.title}</h2><p className="mt-2 max-w-3xl whitespace-pre-wrap text-sm leading-6 text-slate-400">{pack.description || "暂无描述"}</p></div><Button onClick={() => void navigator.clipboard.writeText(`BPH-${pack.id}`)} size="sm" variant="secondary"><Copy className="size-4" />复制分享码</Button></div><div className="mt-5 flex flex-wrap gap-4 text-xs text-slate-500"><span>{pack.beatmapset_ids.length} 个谱面集</span><span>{pack.rating.average?.toFixed(1) ?? "暂无"} 分 · {pack.rating.count} 人</span><span>已解析 {resolved.length}/{pack.beatmapset_ids.length}{resolveProgress < pack.beatmapset_ids.length ? "…" : ""}</span></div></div>
      <div className="grid gap-5 p-6 lg:grid-cols-[1fr_auto]"><div className="max-h-72 space-y-2 overflow-y-auto pr-2">{pack.beatmapset_ids.map((id, index) => { const set = resolved.find((value) => value.id === id); return <div className="flex items-center gap-3 rounded-xl border border-white/[0.06] bg-black/10 px-4 py-3" key={id}><span className="w-6 text-right text-xs text-slate-600">{index + 1}</span><div className="min-w-0"><p className="truncate text-sm text-slate-200">{set?.title ?? `Beatmapset #${id}`}</p><p className="truncate text-xs text-slate-600">{set ? `${set.artist} · ${set.creator} · ${set.beatmaps?.length ?? 0} 难度` : "元数据暂不可用，将以占位条目导入"}</p></div></div>; })}</div><div className="w-full space-y-3 lg:w-60"><label className="flex items-center gap-2 text-sm text-slate-300"><input checked={downloadMissing} className="accent-cyan-300" onChange={(event) => setDownloadMissing(event.target.checked)} type="checkbox" />同时下载缺失谱面</label><Button className="w-full" loading={busy === "import"} onClick={() => void importPack()}><Download className="size-4" />确认导入</Button><div className="flex gap-2"><Button className="flex-1" loading={busy === "favorite"} onClick={() => void run("favorite", async () => { await desktopApi.favoriteBeatmapHubPack(pack.id, !pack.viewer?.favorited); await refreshPack(pack.id); })} variant="secondary"><Heart className={`size-4 ${pack.viewer?.favorited ? "fill-current text-pink-300" : ""}`} />{pack.viewer?.favorited ? "已收藏" : "收藏"}</Button></div><div className="flex justify-center gap-1">{[1,2,3,4,5].map((score) => <button aria-label={`${score} 星`} className="p-1 text-amber-300 disabled:opacity-50" disabled={busy === "rating"} key={score} onClick={() => void run("rating", async () => { await desktopApi.rateBeatmapHubPack(pack.id, score); await refreshPack(pack.id); })} type="button"><Star className={`size-5 ${score <= (pack.viewer?.rating ?? 0) ? "fill-current" : ""}`} /></button>)}</div>{pack.viewer?.can_edit ? <><Button className="w-full" disabled={!selected || !title.trim()} loading={busy === "update"} onClick={() => void run("update", async () => { await desktopApi.updateBeatmapHubPack(pack.id, folderId, title, description); await refreshPack(pack.id); })} variant="secondary">用所选收藏夹更新</Button><Button className="w-full" loading={busy === "delete"} onClick={() => { if (window.confirm("确定永久删除这个 Hub 曲包吗？")) void run("delete", async () => { await desktopApi.deleteBeatmapHubPack(pack.id); setPack(null); setResolved([]); setNotice("曲包已删除"); }); }} variant="ghost"><Trash2 className="size-4" />删除曲包</Button></> : null}</div></div>
    </Card> : <EmptyState icon={<PackageOpen className="size-6" />} title="尚未打开曲包" description="输入分享码，或选择本地收藏夹发布。" />}
    <Card className="p-5"><div className="flex flex-wrap items-center justify-between gap-3"><div className="flex items-center gap-3"><Users className="size-5 text-cyan-200" /><div><h2 className="font-semibold text-white">设备与身份</h2><p className="text-xs text-slate-500">{profile.data?.user.display_name ?? auth.data.display_name} · 当前设备 {auth.data.device_name}</p></div></div><div className="flex gap-2"><Button loading={busy === "link"} onClick={() => void run("link", async () => { const result = await desktopApi.createBeatmapHubDeviceLink(); setLinkToken(result.link_token); })} size="sm" variant="secondary"><Link2 className="size-4" />生成链接码</Button><Button onClick={() => void run("logout", async () => { await desktopApi.logoutBeatmapHub(); await queryClient.invalidateQueries({ queryKey: beatmapHubAuthKey }); })} size="sm" variant="ghost"><LogOut className="size-4" />注销 Session</Button></div></div>{linkToken ? <div className="mt-4 flex items-center gap-2 rounded-xl border border-amber-300/15 bg-amber-300/[0.06] p-3"><code className="min-w-0 flex-1 truncate text-xs text-amber-100">{linkToken}</code><Button onClick={() => void navigator.clipboard.writeText(linkToken)} size="sm" variant="secondary"><Copy className="size-4" />复制</Button></div> : null}<div className="mt-4 space-y-2">{profile.data?.devices.filter((device) => !device.revoked_at).map((device) => <div className="flex items-center justify-between rounded-lg bg-black/10 px-3 py-2" key={device.id}><div><p className="text-sm text-slate-200">{device.device_name}{device.id === profile.data?.current_device_id ? "（当前）" : ""}</p><p className="text-xs text-slate-600">最近使用 {new Date(device.last_seen_at).toLocaleString()}</p></div>{device.id !== profile.data?.current_device_id ? <Button onClick={() => void run("revoke", async () => { await desktopApi.revokeBeatmapHubDevice(device.id); await queryClient.invalidateQueries({ queryKey: beatmapHubProfileKey }); })} size="sm" variant="ghost">撤销</Button> : null}</div>)}</div></Card>
    {notice ? <p className="rounded-xl border border-emerald-300/15 bg-emerald-300/[0.06] px-4 py-3 text-sm text-emerald-100">{notice}</p> : null}
    {error ? <p className="rounded-xl border border-rose-300/15 bg-rose-300/[0.06] px-4 py-3 text-sm text-rose-100" role="alert">{error}</p> : null}
  </div>;
}
