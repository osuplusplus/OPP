import { useState } from "react";
import { Download, RefreshCw } from "lucide-react";
import { desktopApi } from "../../shared/lib/tauri";
import { errorMessage } from "../../shared/lib/format";
import type { BeatmapDownloadProvider, CollectionEntry, CollectionFolderSummary, CollectionPoolSnapshot } from "../../shared/types/osu";
import { DownloadResultActions, useBeatmapDownloads } from "../online-beatmaps/api";
import { useSyncTournamentPool } from "./api";
import { PoolMetadataRepair } from "./PoolMetadataRepair";
import { savedPoolDownloads } from "./model";

export function CollectionPoolActions({ folder, pool, onImportStable }: {
  folder: CollectionFolderSummary; pool: CollectionPoolSnapshot; onImportStable: () => Promise<void>;
}) {
  const downloads = useBeatmapDownloads();
  const sync = useSyncTournamentPool(pool.reference);
  const [preparing, setPreparing] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [provider, setProvider] = useState<BeatmapDownloadProvider | "none" | null>(null);
  const start = async () => {
    setPreparing(true); setNotice(null);
    try {
      // Capture a single folder revision; a concurrent edit asks the user to retry.
      const entries: CollectionEntry[] = [];
      for (let offset = 0; offset < folder.entry_count; offset += 100) {
        const page = await desktopApi.queryCollectionEntries(folder.id, offset, 100, folder.revision);
        entries.push(...page.items);
      }
      const { items, unavailable } = savedPoolDownloads(entries, pool);
      if (unavailable.length) setNotice(`以下谱面缺少资料或不可下载，已跳过：${unavailable.map((id) => `BID ${id}`).join("、")}。可手动同步图池重试补全资料。`);
      if (items.length) await downloads.start(items, { provider: provider ?? downloads.defaultProvider, openAfterDownload: false });
      else if (!unavailable.length) setNotice("没有可下载的谱面。");
    } catch (error) { setNotice(errorMessage(error)); }
    finally { setPreparing(false); }
  };
  const busy = preparing || sync.isPending;
  return <section className="collection-pool-info" aria-label="比赛图池信息">
    <div className="collection-pool-actions"><small>{folder.stable_sync === false ? "仅保存在 OPP" : "已加入 Stable 同步"} · osu!standard</small>
      <button disabled={busy || downloads.state.busy || (provider ?? downloads.defaultProvider) === "none"} onClick={() => void start()}><Download size={15} />下载图池</button>
      <button disabled={busy} onClick={() => { setPreparing(true); void onImportStable().catch((e) => setNotice(errorMessage(e))).finally(() => setPreparing(false)); }}>导入 Stable</button>
      {downloads.state.busy && <button onClick={() => void downloads.cancel()}>取消当前下载</button>}
    </div>
    <details className="collection-pool-settings"><summary>图池设置</summary>
      <p>{[pool.tournament, pool.season, pool.category].filter(Boolean).join(" · ")}</p>
      <p className="collection-muted">同步会替换来源图池，移除旧图及手动添加的图；你的笔记、标签和成绩会保留。下载只保存曲包文件。</p>
      <div className="collection-pool-actions">
      <button disabled={busy} onClick={() => sync.mutate()}><RefreshCw size={15} />{sync.isPending ? "正在同步…" : "同步图池"}</button>
      <span>{downloads.destination || "开始下载时选择目录"}</span>
      <label>下载源 <select aria-label="图池下载源" disabled={downloads.state.busy} value={provider ?? downloads.defaultProvider} onChange={(e) => setProvider(e.target.value as BeatmapDownloadProvider | "none")}>
        <option value="sayobot">小夜 Sayobot（推荐）</option><option value="hinai">Hinai（仅手动）</option><option value="catboy">Catboy</option><option value="nerinyan">Nerinyan</option><option value="none">不使用镜像</option>
      </select></label>
      <button disabled={downloads.state.busy || busy} onClick={() => { void desktopApi.chooseBeatmapDownloadDirectory(downloads.destination || null).then((path) => path ? downloads.saveDestination(path) : undefined).catch((error) => setNotice(errorMessage(error))); }}>更改保存目录</button>
    </div></details>
    {notice && <p role="status">{notice}</p>}
    {sync.error && <p role="alert">{errorMessage(sync.error)}</p>}

    {downloads.state.error && <p role="alert">{downloads.state.error}</p>}
    {downloads.state.busy && downloads.state.progress && <p role="status">当前下载：{downloads.state.progress.processed}/{downloads.state.progress.total} · {downloads.state.progress.current_title || downloads.state.progress.message}</p>}
    {downloads.state.result && <details className="collection-download-result"><summary>{downloads.state.result.cancelled ? "下载已取消" : "下载完成"} · {downloads.state.result.completed + downloads.state.result.skipped} 个曲包{downloads.state.result.failed > 0 ? ` · ${downloads.state.result.failed} 个失败` : ""} · 查看结果</summary>
      {downloads.state.result.failures.map((failure) => <p key={failure.beatmapset_id}>{failure.title}：{failure.message}</p>)}
      <DownloadResultActions result={downloads.state.result} />
    </details>}
    <PoolMetadataRepair folderId={folder.id} pool={pool} />
  </section>;
}
