import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Download } from "lucide-react";
import { AppDialog } from "../../shared/components/AppDialog";
import { Button } from "../../shared/components/ui";
import { useCollectorDetail, useHubDownloadConfig } from "./api";
import { hubError } from "./model";
import type { HubImportTask, useHubImport } from "./useHubImport";

/** osu!Collector 收藏集合详情：本地已有/缺失统计、难度样本与导入入口。 */
export function CollectorDialog({ id, open, onClose, task, importBusy, onImport }: {
  id: number; open: boolean; onClose: () => void;
  task?: HubImportTask; importBusy: boolean;
  onImport: ReturnType<typeof useHubImport>["runCollector"];
}) {
  const navigate = useNavigate();
  const detail = useCollectorDetail(open ? id : null);
  const config = useHubDownloadConfig(open);
  const [downloadChoice, setDownloadChoice] = useState<boolean | null>(null);
  const data = detail.data;
  const available = Boolean(config.data?.root);
  const hasMissing = (data?.missing_sets ?? 0) > 0;
  const download = available && (downloadChoice ?? true) && hasMissing;
  const busy = Boolean(task && !["complete", "error"].includes(task.phase));
  const retry = task?.phase === "error" && Boolean(task.imported);
  const footer = <div className="hub-import-footer">
    {task ? <div role={task.error ? "alert" : "status"} className={task.error ? "hub-error" : "hub-feedback"}><p>{task.message}</p>{task.error ? <p>{task.error}</p> : null}</div> : null}
    <div className="hub-import-action"><div><label className="hub-checkbox"><input type="checkbox" checked={download} disabled={!available || !hasMissing || importBusy || Boolean(task?.imported)} onChange={(event) => setDownloadChoice(event.target.checked)} />同时下载缺失谱面</label>
      {config.isPending ? <p className="hub-muted">正在检查下载配置…</p> : !available ? <p className="hub-muted">{config.error ? "下载配置读取失败，可仅导入。" : "配置 Stable 目录后可自动补齐。"}<button className="hub-text-button" onClick={() => { onClose(); navigate("/settings"); }}>前往设置</button>{config.error ? <button className="hub-text-button" onClick={() => void config.refetch()}>重试</button> : null}</p> : !hasMissing ? <p className="hub-muted">本地已包含全部谱面集，无需下载。</p> : <p className="hub-muted">沿用设置中的下载源、目录和视频选项。</p>}
    </div><Button variant="primary" loading={busy} disabled={importBusy || detail.isPending || task?.phase === "complete" || (retry && !available)} onClick={() => data && void onImport(data.collection.id, data.collection.name, retry || download, config.data)}><Download size={16} />{task?.phase === "complete" ? "已导入收藏夹" : retry ? "重试补齐" : "导入收藏夹"}</Button></div>
  </div>;
  return <AppDialog open={open} onOpenChange={(value) => { if (!value) onClose(); }} overlayProps={{ className: "hub-overlay" }} title={data?.collection.name ?? "osu!Collector 收藏集合"} description={`osu!Collector · ${data?.collection.uploader ?? "…"}`} size="lg" contentClassName="hub-dialog hub-detail" bodyClassName="hub-detail-body" closeLabel="关闭收藏集合详情" footer={footer}>
    <div className="hub-detail-content">
      {detail.isPending ? <p role="status">正在读取收藏集合…</p> : detail.error ? <div className="hub-error" role="alert"><p>{hubError(detail.error)}</p><Button size="sm" onClick={() => void detail.refetch()}>重试</Button></div> : data ? <>
        <p className="hub-description"><span className="hub-source-badge">osu!Collector</span>{data.collection.description || "来自 osu!Collector 的社区收藏集合，导入后以 [OC] 前缀保存为本地收藏夹。"}</p>
        <div className="hub-preview-counts"><div><strong>{data.total_sets}</strong><span>谱面集</span></div><div><strong>{data.local_sets}</strong><span>本地已有</span></div><div><strong>{data.missing_sets}</strong><span>需要下载</span></div></div>
        {data.unsubmitted_beatmaps ? <p className="hub-muted">{data.unsubmitted_beatmaps} 张未上架难度没有在线 ID，不会导入或补齐。</p> : null}
        <section aria-label="难度样本"><div className="hub-section-heading"><h3>难度样本</h3><span className="hub-muted">共 {data.collection.beatmap_count} 张难度，导入时拉取全部</span></div>
          <ol className="hub-track-list">{data.preview.map((beatmap) => <li key={beatmap.beatmap_id}><span className="hub-track-index">{beatmap.difficulty_rating != null ? `${beatmap.difficulty_rating.toFixed(2)}★` : "—"}</span><div><strong>{beatmap.difficulty_name}</strong><p>{beatmap.title} · {beatmap.artist} · {beatmap.creator}</p></div><span className="hub-track-status">{beatmap.local ? "本地已有" : "待下载"}</span></li>)}</ol>
        </section>
      </> : null}
    </div>
  </AppDialog>;
}
