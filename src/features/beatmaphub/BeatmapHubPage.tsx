import { useMemo, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { CheckCircle2, Copy, FileInput, Info, Link2, LogOut, PackageOpen, RefreshCw, Send, ShieldCheck, Star, Users, Globe2, MessageCircle, ThumbsUp } from "lucide-react";
import { PageHeader } from "../../shared/components/PageHeader";
import { Badge, Button, Card, EmptyState } from "../../shared/components/ui";
import { desktopApi } from "../../shared/lib/tauri";
import type { BeatmapHubPack, CommandError, OnlineBeatmapset } from "../../shared/types/osu";
import { useAuthStatus } from "../auth/api";
import { collectionsQueryKey, useCollections } from "../collections/api";
import { resolveDefaultDownloadProvider } from "../online-beatmaps/downloadProvider";
import { beatmapHubAuthKey, beatmapHubProfileKey, beatmapHubRecommendationsKey, useBeatmapHubAuth, useBeatmapHubProfile, useBeatmapHubRecommendations } from "./api";
import { BeatmapHubPackDialog } from "./BeatmapHubPackDialog";

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
  return <Card className="mx-auto max-w-2xl p-5">
    <div className="flex items-center gap-3"><span className="grid size-9 place-items-center rounded-lg bg-cyan-300/10 text-cyan-200"><ShieldCheck className="size-4.5" /></span><div><h2 className="text-base font-semibold text-white">连接 BeatmapHub</h2><p className="text-xs text-slate-500">使用本设备密钥建立独立社区身份，不会改变 osu! 登录。</p></div></div>
    <div className="mt-4 flex gap-2"><Button onClick={() => setMode("create")} size="sm" variant={mode === "create" ? "primary" : "secondary"}>创建新档案</Button><Button onClick={() => setMode("link")} size="sm" variant={mode === "link" ? "primary" : "secondary"}>链接已有档案</Button></div>
    <div className="mt-5 grid gap-4 sm:grid-cols-2">
      {mode === "create" ? <label className="text-xs text-slate-400">显示名<input className="opp-input mt-1.5" maxLength={64} onChange={(event) => setDisplayName(event.target.value)} value={displayName} /></label> : null}
      <label className="text-xs text-slate-400">设备名<input className="opp-input mt-1.5" maxLength={64} onChange={(event) => setDeviceName(event.target.value)} value={deviceName} /></label>
      {mode === "link" ? <label className="text-xs text-slate-400 sm:col-span-2">一次性链接码<input className="opp-input mt-1.5 font-mono" onChange={(event) => setToken(event.target.value)} placeholder="粘贴旧设备生成的 43 位链接码" value={token} /></label> : null}
    </div>
    <Button className="mt-4" disabled={!deviceName.trim() || (mode === "create" ? !displayName.trim() : token.trim().length !== 43)} loading={busy} onClick={() => void submit()} size="sm">{mode === "create" ? "创建并连接" : "链接此设备"}</Button>
    {error ? <p className="mt-4 text-sm text-rose-200" role="alert">{error}</p> : null}
    <p className="mt-5 text-xs leading-5 text-slate-600">私钥仅保存在系统安全存储中。若所有已登记设备均丢失，档案无法人工找回。</p>
  </Card>;
}

function PackArtwork({ pack }: { pack: BeatmapHubPack }) {
  const coverIds = pack.beatmapset_ids.slice(0, 3);
  return (
    <div aria-hidden="true" className="relative h-28 overflow-hidden bg-[var(--surface-interactive)]">
      {coverIds.length ? <div className="grid size-full grid-cols-3 gap-px bg-black/20">{coverIds.map((id, index) => (
        <span className="relative block overflow-hidden bg-[var(--surface-interactive)]" key={id}>
          <img
            alt=""
            className="absolute inset-0 size-full object-cover transition-transform duration-300 group-hover:scale-[1.03]"
            loading="lazy"
            onError={(event) => { event.currentTarget.style.display = "none"; }}
            src={`https://assets.ppy.sh/beatmaps/${id}/covers/cover.jpg`}
            style={{ opacity: index === 1 ? 0.92 : 0.76 }}
          />
        </span>
      ))}</div> : <div className="grid size-full place-items-center"><PackageOpen className="size-8 text-slate-600" /></div>}
      <div className="absolute inset-0 bg-gradient-to-t from-[var(--surface-panel-strong)] via-transparent to-black/10" />
      <span className="theme-media-copy absolute bottom-2 left-3 rounded-md border border-white/10 px-2 py-1 text-[10px] font-semibold text-white backdrop-blur-md" style={{ backgroundColor: "rgba(0,0,0,.55)" }}>{pack.beatmapset_ids.length} 个谱面集</span>
    </div>
  );
}

function PubPackCard({ pack, onOpen }: { pack: BeatmapHubPack; onOpen: () => void }) {
  return (
    <button className="group overflow-hidden rounded-xl border border-white/[0.09] bg-[var(--surface-panel)] text-left transition-colors hover:border-[var(--theme-primary)]/40 hover:bg-[var(--surface-panel-strong)]" onClick={onOpen} type="button">
      <PackArtwork pack={pack} />
      <div className="p-4">
        <div className="flex items-center justify-between gap-3"><Badge tone="cyan">BPH-{pack.id}</Badge><span className="text-[10px] text-slate-500">{new Date(pack.updated_at).toLocaleDateString()}</span></div>
        <h3 className="mt-3 truncate text-[15px] font-semibold text-white group-hover:text-cyan-100">{pack.title}</h3>
        <p className="mt-1 truncate text-xs text-slate-500">由 {pack.owner.display_name} 整理</p>
        <p className="mt-2 line-clamp-2 min-h-9 text-xs leading-[18px] text-slate-400">{pack.description || "这个曲包暂时没有说明。"}</p>
        <div className="mt-3 flex items-center gap-3 border-t border-white/[0.07] pt-3 text-[10px] text-slate-500">
          {pack.stars_min != null && pack.stars_max != null ? <span className="inline-flex items-center gap-1"><Star aria-hidden="true" className="size-3 text-amber-300" />{pack.stars_min.toFixed(1)}–{pack.stars_max.toFixed(1)}★</span> : null}
          <span className="inline-flex items-center gap-1"><Star className="size-3 text-amber-300" />{pack.rating.average?.toFixed(1) ?? "—"}<span className="text-slate-600">({pack.rating.count})</span></span>
          <span className="inline-flex items-center gap-1"><ThumbsUp className="size-3" />{pack.likes.count}</span>
          <span className="inline-flex items-center gap-1"><MessageCircle className="size-3" />{pack.comments.count}</span>
          <span className="ml-auto inline-flex items-center gap-1 text-emerald-300"><Globe2 className="size-3" />公开</span>
        </div>
      </div>
    </button>
  );
}

function PubBrowser({ onOpen }: { onOpen: (id: string) => void }) {
  const queryClient = useQueryClient();
  const recommendations = useBeatmapHubRecommendations();
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<BeatmapHubPack[] | null>(null);
  const [searching, setSearching] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const search = async () => {
    if (!query.trim() || typeof desktopApi.searchBeatmapHubPacks !== "function") return;
    setSearching(true);
    try { setResults(await desktopApi.searchBeatmapHubPacks(query)); } finally { setSearching(false); }
  };
  const refresh = async () => {
    setRefreshing(true);
    try {
      const packs = await desktopApi.getBeatmapHubRecommendations(20, true);
      queryClient.setQueryData(beatmapHubRecommendationsKey, packs);
      setResults(null);
    } finally { setRefreshing(false); }
  };
  if (recommendations.isLoading) return <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">{Array.from({ length: 6 }, (_, index) => <div className="h-72 animate-pulse rounded-xl border border-white/[0.06] bg-white/[0.03]" key={index} />)}</div>;
  if (recommendations.error) return <Card className="p-6"><div className="flex items-center justify-between gap-4"><div><h2 className="font-semibold text-white">Pub 暂时不可用</h2><p className="mt-1 text-sm text-slate-500">公开曲包加载失败，请检查网络后重试。</p></div><Button onClick={() => void recommendations.refetch()} variant="secondary"><RefreshCw className="size-4" />重试</Button></div></Card>;
  if (!recommendations.data?.length) return <EmptyState icon={<Globe2 className="size-6" />} title="还没有公开曲包" description="成为第一个把收藏夹分享给社区的人吧。" />;
  const packs = (results ?? recommendations.data).filter((pack) => !pack.is_private);
  return <><div className="mb-5 flex gap-2"><input className="opp-input flex-1" onChange={(event) => setQuery(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter") void search(); }} placeholder="搜索曲包标题、描述或作者" value={query} /><Button disabled={!query.trim()} loading={searching} onClick={() => void search()} size="sm" variant="secondary">搜索</Button><Button aria-label="刷新 Pub" loading={refreshing} onClick={() => void refresh()} size="icon" title="刷新 Pub" variant="secondary"><RefreshCw className="size-4" /></Button></div><div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">{packs.map((pack) => <PubPackCard key={pack.id} onOpen={() => onOpen(pack.id)} pack={pack} />)}</div></>;
}

export function BeatmapHubPage() {
  const queryClient = useQueryClient();
  const auth = useBeatmapHubAuth();
  const osuAuth = useAuthStatus();
  const collections = useCollections();
  const profile = useBeatmapHubProfile(Boolean(auth.data?.connected));
  const [view, setView] = useState<"pub" | "open" | "publish">("open");
  const writable = useMemo(() => (collections.data?.folders ?? []).filter((folder) => !folder.read_only && folder.source !== "lazer"), [collections.data]);
  const [shareCode, setShareCode] = useState("");
  const [pack, setPack] = useState<BeatmapHubPack | null>(null);
  const [resolved, setResolved] = useState<OnlineBeatmapset[]>([]);
  const [resolveProgress, setResolveProgress] = useState(0);
  const [folderId, setFolderId] = useState("");
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [downloadMissing, setDownloadMissing] = useState(true);
  const [isPrivate, setIsPrivate] = useState(false);
  const [linkToken, setLinkToken] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [publishedCode, setPublishedCode] = useState<string | null>(null);

  const refreshPack = async (id: string) => {
    const preview = await desktopApi.previewBeatmapHubPack(id);
    const value = preview.pack;
    setNotice(`本地已有 ${preview.locally_available_ids.length} 个谱面集，缺失 ${preview.missing_ids.length} 个。`);
    setPack(value); setShareCode(`BPH-${value.id}`); setResolveProgress(0);
    setTitle(value.title); setDescription(value.description); setIsPrivate(value.is_private);
    setResolved(await resolveSets(value.beatmapset_ids, setResolveProgress));
  };
  const run = async (key: string, action: () => Promise<void>) => {
    setBusy(key); setError(null); setNotice(null);
    try { await action(); } catch (caught) { setError(errorText(caught)); } finally { setBusy(null); }
  };
  const openPack = () => run("open", async () => { await refreshPack(shareCode); });
  const publish = () => run("publish", async () => {
    const result = await desktopApi.publishBeatmapHubPack(folderId, title, description, isPrivate);
    await navigator.clipboard?.writeText(`BPH-${result.id}`);
    setShareCode(result.id); await refreshPack(result.id);
    setPublishedCode(`BPH-${result.id}`);
    setNotice(`发布成功，包含 ${result.included} 个谱面集${result.skipped ? `，跳过 ${result.skipped} 个无 ID 条目` : ""}。分享码已复制。`);
  });
  const importArchiveForPublish = () => run("archive", async () => {
    const path = await open({ multiple: false, filters: [{ name: "谱面压缩包", extensions: ["osz", "zip"] }] });
    if (typeof path !== "string") return;
    const folder = await desktopApi.importCollectionArchive(path);
    await queryClient.invalidateQueries({ queryKey: collectionsQueryKey });
    setFolderId(folder.id); setTitle(folder.name); setView("publish"); setPublishedCode(null);
    setNotice(`已导入“${folder.name}”，确认可见性与说明后即可发布。`);
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
  if (!auth.data?.has_identity) return <div className="space-y-6"><PageHeader title="BeatmapHub" actions={<Badge tone="cyan"><Globe2 className="size-3" />Pub</Badge>} /><PubBrowser onOpen={(id) => { setShareCode(id); setView("open"); void run("open", async () => { await refreshPack(id); }); }} /><IdentitySetup defaultName={osuAuth.data?.username ?? "Player"} defaultDevice={auth.data?.device_name ?? "Desktop PC"} /></div>;

  const selected = writable.find((folder) => folder.id === folderId);
  return <div className="beatmaphub-page space-y-6">
    <PageHeader title="BeatmapHub" actions={<div className="flex items-center gap-1 border-b border-white/[0.08]"><button className={`px-3 py-2 text-xs font-semibold ${view === "pub" ? "text-cyan-100 shadow-[inset_0_-2px_0_var(--theme-primary)]" : "text-slate-500"}`} onClick={() => setView("pub")} type="button"><Globe2 className="mr-1 inline size-3.5" />Pub</button><button className={`px-3 py-2 text-xs font-semibold ${view === "open" ? "text-cyan-100 shadow-[inset_0_-2px_0_var(--theme-primary)]" : "text-slate-500"}`} onClick={() => setView("open")} type="button"><PackageOpen className="mr-1 inline size-3.5" />打开</button><button className={`px-3 py-2 text-xs font-semibold ${view === "publish" ? "text-cyan-100 shadow-[inset_0_-2px_0_var(--theme-primary)]" : "text-slate-500"}`} onClick={() => setView("publish")} type="button"><Send className="mr-1 inline size-3.5" />发布</button></div>} />
    {view === "pub" ? <PubBrowser onOpen={(id) => { setShareCode(id); setView("open"); void run("open", async () => { await refreshPack(id); }); }} /> : null}
    {view !== "pub" ? <Card className="border-cyan-300/15 bg-cyan-300/[0.045] p-4">
      <div className="flex items-start gap-3"><Info className="mt-0.5 size-4 shrink-0 text-cyan-200" /><div><p className="text-sm font-medium text-cyan-100">使用提示</p><p className="mt-1 text-xs leading-5 text-slate-400">分享码只包含谱面集清单，不上传本地谱面文件。导入会新建本地收藏夹；勾选“同时下载缺失谱面”时，需要先在设置中配置有效的 osu!stable 目录，下载源、保存位置及是否包含视频均沿用“设置 → 谱面下载”。</p></div></div>
    </Card> : null}
    {!auth.data.connected ? <><Card className="flex items-center justify-between gap-4 p-5"><div><p className="font-medium text-amber-100">Hub Session 已过期</p><p className="mt-1 text-xs text-slate-500">设备私钥仍安全保留，可以直接重新验证。</p></div><Button loading={busy === "login"} onClick={() => void run("login", async () => { await desktopApi.loginBeatmapHub(); await queryClient.invalidateQueries({ queryKey: beatmapHubAuthKey }); })}><RefreshCw className="size-4" />重新连接</Button></Card><details className="rounded-xl border border-white/[0.08] bg-black/10 p-4"><summary className="cursor-pointer text-sm text-slate-400">设备已撤销或私钥不可用？重新建立身份</summary><div className="mt-4"><IdentitySetup defaultName={auth.data.display_name ?? osuAuth.data?.username ?? "Player"} defaultDevice={auth.data.device_name ?? "Desktop PC"} /></div></details></> : null}
    {view !== "pub" ? <div className="grid gap-5 xl:grid-cols-2">
      {view === "open" ? <Card className="p-5"><div className="flex items-center gap-2"><PackageOpen className="size-5 text-cyan-200" /><h2 className="font-semibold text-white">打开社区曲包</h2></div><p className="mt-1 text-xs text-slate-500">输入 6 位分享码，支持 BPH- 前缀。</p><div className="mt-4 flex gap-2"><input className="opp-input font-mono uppercase" onChange={(event) => setShareCode(event.target.value)} placeholder="BPH-7K3N9A" value={shareCode} /><Button disabled={!shareCode.trim()} loading={busy === "open"} onClick={() => void openPack()}>打开</Button></div></Card> : null}
      {view === "publish" ? <Card className="p-5"><div className="flex items-center justify-between gap-3"><div className="flex items-center gap-2"><Send className="size-5 text-violet-200" /><h2 className="font-semibold text-white">发布本地收藏夹</h2></div><Button loading={busy === "archive"} onClick={() => void importArchiveForPublish()} size="sm" variant="secondary"><FileInput className="size-4" />选择 .osz/.zip</Button></div><div className="mt-4 grid gap-3"><select className="opp-input" onChange={(event) => { const id = event.target.value; setFolderId(id); setPublishedCode(null); const folder = writable.find((value) => value.id === id); if (folder) setTitle(folder.name); }} value={folderId}><option value="">选择收藏夹</option>{writable.map((folder) => <option key={folder.id} value={folder.id}>{folder.name}（{new Set(folder.entries.map((entry) => entry.beatmapset_id).filter(Boolean)).size} 个谱面集）</option>)}</select><input className="opp-input" maxLength={120} onChange={(event) => setTitle(event.target.value)} placeholder="曲包标题" value={title} /><textarea className="opp-input min-h-20 resize-y" maxLength={2000} onChange={(event) => setDescription(event.target.value)} placeholder="用途、难度或玩法说明（可选）" value={description} /><label className="flex items-center gap-2 text-sm text-slate-300"><input checked={!isPrivate} className="accent-cyan-300" onChange={() => setIsPrivate(false)} type="radio" />公开到 Pub</label><label className="flex items-center gap-2 text-sm text-slate-300"><input checked={isPrivate} className="accent-cyan-300" onChange={() => setIsPrivate(true)} type="radio" />私有，仅凭分享码访问</label><Button disabled={!selected || !title.trim()} loading={busy === "publish"} onClick={() => void publish()}>发布并复制分享码</Button>{publishedCode ? <div className="flex items-center gap-3 border border-emerald-300/20 bg-emerald-300/[0.06] p-3"><CheckCircle2 className="size-5 shrink-0 text-emerald-300" /><div className="min-w-0 flex-1"><p className="text-sm font-semibold text-emerald-100">已经发布</p><p className="truncate font-mono text-xs text-emerald-200">{publishedCode}</p></div><Button onClick={() => void navigator.clipboard.writeText(publishedCode)} size="sm" variant="secondary"><Copy className="size-4" />复制</Button></div> : null}</div></Card> : null}
    </div> : null}
    <Card className="p-5"><div className="flex flex-wrap items-center justify-between gap-3"><div className="flex items-center gap-3"><Users className="size-5 text-cyan-200" /><div><h2 className="font-semibold text-white">设备与身份</h2><p className="text-xs text-slate-500">{profile.data?.user.display_name ?? auth.data.display_name} · 当前设备 {auth.data.device_name}</p></div></div><div className="flex gap-2"><Button loading={busy === "link"} onClick={() => void run("link", async () => { const result = await desktopApi.createBeatmapHubDeviceLink(); setLinkToken(result.link_token); })} size="sm" variant="secondary"><Link2 className="size-4" />生成链接码</Button><Button onClick={() => void run("logout", async () => { await desktopApi.logoutBeatmapHub(); await queryClient.invalidateQueries({ queryKey: beatmapHubAuthKey }); })} size="sm" variant="ghost"><LogOut className="size-4" />注销 Session</Button></div></div>{linkToken ? <div className="mt-4 flex items-center gap-2 rounded-xl border border-amber-300/15 bg-amber-300/[0.06] p-3"><code className="min-w-0 flex-1 truncate text-xs text-amber-100">{linkToken}</code><Button onClick={() => void navigator.clipboard.writeText(linkToken)} size="sm" variant="secondary"><Copy className="size-4" />复制</Button></div> : null}<div className="mt-4 space-y-2">{profile.data?.devices.filter((device) => !device.revoked_at).map((device) => <div className="flex items-center justify-between rounded-lg bg-black/10 px-3 py-2" key={device.id}><div><p className="text-sm text-slate-200">{device.device_name}{device.id === profile.data?.current_device_id ? "（当前）" : ""}</p><p className="text-xs text-slate-600">最近使用 {new Date(device.last_seen_at).toLocaleString()}</p></div>{device.id !== profile.data?.current_device_id ? <Button onClick={() => void run("revoke", async () => { await desktopApi.revokeBeatmapHubDevice(device.id); await queryClient.invalidateQueries({ queryKey: beatmapHubProfileKey }); })} size="sm" variant="ghost">撤销</Button> : null}</div>)}</div></Card>
    <BeatmapHubPackDialog busy={busy} currentUserId={profile.data?.user.id ?? auth.data.user_id} downloadMissing={downloadMissing} onClose={() => { setPack(null); setResolved([]); }} onDownloadMissingChange={setDownloadMissing} onFavorite={() => void run("favorite", async () => { if (!pack) return; await desktopApi.favoriteBeatmapHubPack(pack.id, !pack.viewer?.favorited); await refreshPack(pack.id); })} onImport={() => void importPack()} onLike={() => void run("like", async () => { if (!pack) return; await desktopApi.likeBeatmapHubPack(pack.id, !pack.viewer?.liked); await refreshPack(pack.id); })} onRate={(score) => void run("rating", async () => { if (!pack) return; await desktopApi.rateBeatmapHubPack(pack.id, score); await refreshPack(pack.id); })} onUpdate={() => void run("update", async () => { if (!pack) return; await desktopApi.updateBeatmapHubPack(pack.id, folderId, title, description, isPrivate); await refreshPack(pack.id); })} pack={pack} resolved={resolved} resolveProgress={resolveProgress} />
    {notice ? <p className="rounded-xl border border-emerald-300/15 bg-emerald-300/[0.06] px-4 py-3 text-sm text-emerald-100">{notice}</p> : null}
    {error ? <p className="rounded-xl border border-rose-300/15 bg-rose-300/[0.06] px-4 py-3 text-sm text-rose-100" role="alert">{error}</p> : null}
  </div>;
}
