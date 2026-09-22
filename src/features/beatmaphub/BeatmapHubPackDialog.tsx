import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { Copy, Download, Heart, Pencil, Star, ThumbsUp, Trash2 } from "lucide-react";
import { AppDialog } from "../../shared/components/AppDialog";
import { Button } from "../../shared/components/ui";
import type { BeatmapHubPack, BeatmapHubPackPreview, OnlineBeatmapset } from "../../shared/types/osu";
import { hubApi, hubPackKey, hubSearchKey, beatmapHubRecommendationsKey, useHubDownloadConfig, useHubMetadata } from "./api";
import { HubCover } from "./HubBrowser";
import { HubComments } from "./HubComments";
import { copyHubCode, hubError, type CommentDraft } from "./model";
import type { HubImportTask, useHubImport } from "./useHubImport";

export function BeatmapHubPackDialog({ preview, open, onClose, onCloseAutoFocus, connected, userId, onConnect, onEdit, onDeleted, task, importBusy, onImport, draft, onDraft }: {
  preview: BeatmapHubPackPreview; open: boolean; onClose: () => void; connected: boolean; userId?: string | null;
  onCloseAutoFocus: (event: Event) => void;
  onConnect: () => void; onEdit: (pack: BeatmapHubPack) => void; onDeleted: () => void;
  task?: HubImportTask; importBusy: boolean; onImport: ReturnType<typeof useHubImport>["run"];
  draft: CommentDraft; onDraft: (draft: CommentDraft) => void;
}) {
  const client = useQueryClient();
  const navigate = useNavigate();
  const pack = preview.pack;
  const metadata = useHubMetadata(pack);
  const config = useHubDownloadConfig(open);
  const [downloadChoice, setDownloadChoice] = useState<boolean | null>(null);
  const [notice, setNotice] = useState("");
  const [confirmDelete, setConfirmDelete] = useState(false);
  const available = Boolean(config.data?.root);
  const hasMissing = preview.missing_ids.length > 0;
  const download = available && (downloadChoice ?? true) && hasMissing;
  const resolved = metadata.data ?? [];
  const sets = new Map<number, OnlineBeatmapset>(resolved.map((set) => [set.id, set]));
  const unresolved = pack.beatmapset_ids.filter((id) => !sets.get(id)?.beatmaps?.length).length;
  const operation = useMutation({ mutationFn: async (action: "favorite" | "like" | "delete" | number) => {
    if (!connected) { onConnect(); return; }
    if (typeof action === "number") await hubApi.rate(pack.id, action);
    else if (action === "favorite") await hubApi.favorite(pack.id, !pack.viewer?.favorited);
    else if (action === "like") await hubApi.like(pack.id, !pack.viewer?.liked);
    else { await hubApi.deletePack(pack.id); setConfirmDelete(false); onDeleted(); }
    void client.invalidateQueries({ queryKey: [...hubPackKey, pack.id] });
    void client.invalidateQueries({ queryKey: hubSearchKey });
    void client.invalidateQueries({ queryKey: beatmapHubRecommendationsKey });
  } });
  const busy = operation.isPending;
  const retry = task?.phase === "error" && Boolean(task.imported);
  const footer = <div className="hub-import-footer">
    {task ? <div role={task.error ? "alert" : "status"} className={task.error ? "hub-error" : "hub-feedback"}><p>{task.message}</p>{task.error ? <p>{task.error}</p> : null}</div> : null}
    <div className="hub-import-action"><div><label className="hub-checkbox"><input type="checkbox" checked={download} disabled={!available || !hasMissing || importBusy || Boolean(task?.imported)} onChange={(event) => setDownloadChoice(event.target.checked)} />同时下载缺失谱面</label>
      {config.isPending ? <p className="hub-muted">正在检查下载配置…</p> : !available ? <p className="hub-muted">{config.error ? "下载配置读取失败，可仅导入。" : "配置 Stable 目录后可自动补齐。"}<button className="hub-text-button" onClick={() => { onClose(); navigate("/settings"); }}>前往设置</button>{config.error ? <button className="hub-text-button" onClick={() => void config.refetch()}>重试</button> : null}</p> : !hasMissing ? <p className="hub-muted">本地已包含全部谱面集，无需下载。</p> : <p className="hub-muted">沿用设置中的下载源、目录和视频选项。</p>}
    </div><Button variant="primary" loading={Boolean(task && !["complete", "error"].includes(task.phase))} disabled={importBusy || !metadata.isSuccess || task?.phase === "complete" || (retry && !available)} onClick={() => void onImport(pack.id, pack.title, resolved, retry || download, config.data)}><Download size={16} />{task?.phase === "complete" ? "已导入收藏夹" : retry ? "重试补齐" : "导入收藏夹"}</Button></div>
  </div>;
  return <>
    <AppDialog open={open} onOpenChange={(value) => { if (!value && !busy) onClose(); }} onCloseAutoFocus={onCloseAutoFocus} overlayProps={{ className: "hub-overlay" }} title={pack.title} description={`${pack.owner.display_name} · ${pack.is_private ? "仅凭分享码访问" : "公开曲包"} · BPH-${pack.id}`} size="lg" contentClassName="hub-dialog hub-detail" bodyClassName="hub-detail-body" closeLabel="关闭曲包详情" closeDisabled={busy} footer={footer}>
      <HubCover ids={pack.beatmapset_ids} className="hub-detail-cover" />
      <div className="hub-detail-content">
        {pack.description ? <p className="hub-description">{pack.description}</p> : null}
        <div className="hub-preview-counts"><div><strong>{pack.beatmapset_ids.length}</strong><span>谱面集</span></div><div><strong>{preview.locally_available_ids.length}</strong><span>本地已有</span></div><div><strong>{preview.missing_ids.length}</strong><span>需要下载</span></div></div>
        <div className="hub-inline-actions"><Button size="sm" disabled={busy} aria-pressed={Boolean(pack.viewer?.favorited)} onClick={() => operation.mutate("favorite")}><Heart size={15} fill={pack.viewer?.favorited ? "currentColor" : "none"} />{pack.viewer?.favorited ? "已收藏" : "收藏"}</Button><Button size="sm" disabled={busy} aria-pressed={Boolean(pack.viewer?.liked)} onClick={() => operation.mutate("like")}><ThumbsUp size={15} />{pack.viewer?.liked ? "已点赞" : "点赞"} {pack.likes?.count ?? 0}</Button><Button size="sm" onClick={() => void copyHubCode(`BPH-${pack.id}`).then((ok) => setNotice(ok ? "分享码已复制。" : `无法自动复制，请手动复制：BPH-${pack.id}`))}><Copy size={15} />分享</Button>
          {connected && pack.viewer?.can_edit ? <><Button size="sm" disabled={busy} onClick={() => onEdit(pack)}><Pencil size={15} />编辑曲包</Button><Button size="sm" variant="ghost" disabled={busy || importBusy} onClick={() => { operation.reset(); setConfirmDelete(true); }}><Trash2 size={15} />删除曲包</Button></> : null}
        </div>
        <div className="hub-rating"><span>社区评分 {pack.rating.average?.toFixed(1) ?? "暂无"} · {pack.rating.count} 人</span><div aria-label="为曲包评分">{[1, 2, 3, 4, 5].map((score) => <button key={score} aria-label={`${score} 星`} aria-pressed={pack.viewer?.rating === score} disabled={busy} onClick={() => operation.mutate(score)}><Star size={19} fill={score <= (pack.viewer?.rating ?? 0) ? "currentColor" : "none"} /></button>)}</div></div>
        {notice ? <p role="status" className="hub-feedback">{notice}</p> : null}
        {operation.error && !confirmDelete ? <p role="alert" className="hub-error">{hubError(operation.error)}</p> : null}
        <section aria-label="曲包内容"><div className="hub-section-heading"><h3>曲包内容</h3><span className="hub-muted">{metadata.isPending ? "正在解析谱面…" : `${resolved.length}/${pack.beatmapset_ids.length} 个元数据已加载`}</span></div>
          {metadata.error ? <div className="hub-error" role="alert">解析失败<Button size="sm" onClick={() => void metadata.refetch()}>重试解析</Button></div> : metadata.isSuccess && unresolved ? <p className="hub-muted">{unresolved} 个谱面集的完整元数据暂不可用，将作为占位条目导入。</p> : null}
          <ol className="hub-track-list">{pack.beatmapset_ids.map((id, index) => { const set = sets.get(id); return <li key={id}><span className="hub-track-index">{String(index + 1).padStart(2, "0")}</span><div><strong>{set?.title ?? `Beatmapset #${id}`}</strong><p>{set ? `${set.artist} · ${set.creator}` : metadata.isPending ? "正在加载…" : "元数据暂不可用 · 占位条目"}</p></div><span className="hub-track-status">{preview.locally_available_ids.includes(id) ? "本地已有" : "待下载"}</span></li>; })}</ol>
        </section>
        <HubComments key={pack.id} id={pack.id} userId={userId} canEdit={Boolean(pack.viewer?.can_edit)} connected={connected} onConnect={onConnect} draft={draft} onDraft={onDraft} />
      </div>
    </AppDialog>
    <AppDialog open={confirmDelete} onOpenChange={(value) => { if (!busy) setConfirmDelete(value); }} overlayProps={{ className: "hub-overlay" }} title="删除这个曲包？" description={`“${pack.title}”的分享码将失效，本地收藏夹不会删除。`} contentClassName="hub-dialog" size="sm" closeDisabled={busy} footer={<><Button disabled={busy} onClick={() => setConfirmDelete(false)}>保留曲包</Button><Button variant="danger" loading={busy} onClick={() => operation.mutate("delete")}>确认删除</Button></>}><p>此操作无法撤销。</p>{operation.error ? <p role="alert" className="hub-error">{hubError(operation.error)}</p> : null}</AppDialog>
  </>;
}
