import { useQuery } from "@tanstack/react-query";
import { useContext, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { Link } from "react-router-dom";
import { FileVideo, FolderOpen, Library, ListVideo, RefreshCw } from "lucide-react";
import { desktopApi } from "../../shared/lib/tauri";
import type { GameMediaItem } from "../../shared/types/osu";
import { inspectLibraryReplay, replayInfoKey, useReplayWorkspace } from "./api";
import { matchesReplay, replayBlockReason, replayName, type RenderProvider } from "./model";
import { StudioPanelsContext } from "./studioPanels";
import { StudioDropdown } from "./StudioDropdown";

export function ReplayIdentity({ provider, children }: { provider: RenderProvider; children?: ReactNode }) {
  const { client, replayPath, replayInfo, inspectError, inspecting, refresh } = useReplayWorkspace();
  const resource = replayInfo?.beatmap_resource_id;
  const artwork = useQuery({ queryKey: ["replay-studio-art", client, resource], queryFn: () => desktopApi.getLocalBeatmapBackground(client, resource!, "stage"), enabled: Boolean(resource), staleTime: Infinity });
  const reason = replayBlockReason(replayPath, replayInfo, provider, inspectError);
  return <section className="studio-identity">
    {artwork.data ? <img className="studio-artwork" src={artwork.data} alt="" /> : null}
    <div className="studio-identity-copy"><span className="studio-eyebrow">{provider === "danser" ? "LOCAL RENDER" : provider === "ordr" ? "CLOUD RENDER" : "REPLAY PREVIEW"}</span>
      <h1>{replayInfo?.beatmap_title ?? (replayPath ? replayName(replayPath) : "让每一次精彩，重新上演。")}</h1>
      <p>{replayInfo?.username || "选择一份回放，开始你的创作"}{replayInfo ? ` · ${replayInfo.ruleset || "osu"}` : ""}</p>
      {replayPath ? <small title={replayPath}>{replayName(replayPath)}</small> : null}
      {reason ? <div className="studio-readiness" role="status">{reason}</div> : <div className="studio-readiness is-ready">素材已就绪</div>}
      {replayPath && (inspectError || (replayInfo && !replayInfo.beatmap_resource_id)) ? <div className="studio-recovery"><Link to="/local/maps">扫描本地谱面</Link><Link to="/online/beatmaps">查找并安装谱面</Link><button disabled={inspecting} onClick={() => void refresh()}><RefreshCw />重新匹配</button></div> : null}
      {children}
    </div>
  </section>;
}

function ReplayRow({ item, query }: { item: GameMediaItem; query: string }) {
  const workspace = useReplayWorkspace();
  const info = useQuery({ queryKey: replayInfoKey(workspace.client, item.path), queryFn: ({ signal }) => inspectLibraryReplay(workspace.client, item.path, signal), staleTime: 30_000, retry: false });
  if (!matchesReplay(item, query, info.data)) return null;
  return <div className="studio-library-row" data-current={workspace.replayPath === item.path}>
    <input type="checkbox" aria-label={`勾选 ${replayName(item.path)}`} checked={workspace.checked.includes(item.path)} onChange={() => workspace.toggleChecked(item.path)} />
    <button onClick={() => workspace.select(item.path)} aria-pressed={workspace.replayPath === item.path}><FileVideo /><span><strong>{info.data?.beatmap_title || replayName(item.path)}</strong><small>{info.error ? "文件无法读取" : info.data?.username || replayName(item.path)} · {workspace.imported.some((entry) => entry.path === item.path) ? "手动选择" : "本地扫描"}</small></span></button>
  </div>;
}

export function ReplayLibrary({ open, onOpenChange, chooseFiles }: { open: boolean; onOpenChange: (value: boolean) => void; chooseFiles: () => void }) {
  const workspace = useReplayWorkspace();
  const [query, setQuery] = useState("");
  const [limit, setLimit] = useState(40);
  const visible = query.trim() ? workspace.items.filter((item) => matchesReplay(item, query)).slice(0, limit) : workspace.items.slice(0, limit);
  return <StudioDropdown open={open} onOpenChange={onOpenChange} trigger={<><Library />素材库<span>{workspace.items.length}</span></>} title="回放素材库" description="当前回放在三个渲染入口间共享；勾选多项可加入 Danser 队列。" closeLabel="关闭素材库">
    <div className="studio-library-tools"><button onClick={chooseFiles}><FolderOpen />选择回放文件</button><button aria-label="刷新素材库" onClick={() => void workspace.refresh()}><RefreshCw /></button></div>
    <input className="studio-search" aria-label="搜索回放" placeholder="搜索文件名或路径…" value={query} onChange={(event) => { setQuery(event.target.value); setLimit(40); }} />
    <div className="studio-library-list">{visible.map((item) => <ReplayRow key={item.path} item={item} query="" />)}{!visible.length ? <p>{workspace.loading ? "正在读取素材…" : "暂无匹配回放，可从任意目录选择 .osr 文件。"}</p> : null}{workspace.items.length > limit ? <button onClick={() => setLimit((value) => value + 40)}>显示更多</button> : null}</div>
    <footer>{workspace.items.length} 份素材 · 已勾选 {workspace.checked.length} 份<span>仅本次运行保留</span></footer>
  </StudioDropdown>;
}

export function StudioTasks({ provider, title, children }: { provider: RenderProvider; title: string; children: ReactNode }) {
  const { host, provider: activeProvider } = useContext(StudioPanelsContext);
  const [open, setOpen] = useState(false);
  if (!host || provider !== activeProvider) return null;
  return createPortal(<StudioDropdown open={open} onOpenChange={setOpen} trigger={<><ListVideo />任务与输出</>} title="任务与输出" description={title} closeLabel="关闭任务与输出"><div className="studio-task-content">{children}</div></StudioDropdown>, host);
}
