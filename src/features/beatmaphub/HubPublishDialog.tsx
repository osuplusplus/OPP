import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { CheckCircle2, Copy, FileInput } from "lucide-react";
import { AppDialog } from "../../shared/components/AppDialog";
import { Button } from "../../shared/components/ui";
import type { BeatmapHubPack } from "../../shared/types/osu";
import { hubApi, hubPackKey, hubSearchKey, beatmapHubRecommendationsKey, useHubCollections } from "./api";
import { copyHubCode, hubError } from "./model";

export function HubPublishDialog({ open: visible, edit, connected, onConnect, onClose, onOpenPack }: {
  open: boolean; edit: BeatmapHubPack | null; connected: boolean; onConnect: () => void; onClose: () => void; onOpenPack: (id: string) => void;
}) {
  const client = useQueryClient();
  const collections = useHubCollections(visible);
  const folders = (collections.data?.folders ?? []).filter((folder) => !folder.read_only && folder.source !== "lazer");
  const [folderId, setFolderId] = useState("");
  const [title, setTitle] = useState(edit?.title ?? "");
  const [description, setDescription] = useState(edit?.description ?? "");
  const [isPrivate, setIsPrivate] = useState(edit?.is_private ?? false);
  const [published, setPublished] = useState<{ id: string; included?: number; skipped?: number } | null>(null);
  const [notice, setNotice] = useState("");
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  const selected = folders.find((folder) => folder.id === folderId);
  const archive = useMutation({ mutationFn: async () => {
    const path = await open({ multiple: false, filters: [{ name: "谱面压缩包", extensions: ["osz", "zip"] }] });
    if (typeof path !== "string") return;
    const folder = await hubApi.importArchive(path);
    await client.invalidateQueries({ queryKey: ["collections"] });
    setFolderId(folder.id); if (!edit) setTitle(folder.name);
    setNotice(`“${folder.name}”已导入本地收藏夹，确认信息后即可${edit ? "更新" : "发布"}。`);
  } });
  const submit = useMutation({ mutationFn: async () => {
    if (!connected) { onConnect(); return; }
    if (edit) {
      await hubApi.update(edit.id, folderId, title.trim(), description, isPrivate);
      setPublished({ id: edit.id });
    } else {
      const result = await hubApi.publish(folderId, title.trim(), description, isPrivate);
      setPublished(result);
      const copied = await copyHubCode(`BPH-${result.id}`);
      setNotice(copied ? "分享码已复制。" : "发布成功。无法自动复制，请手动复制下方分享码。");
    }
    void client.invalidateQueries({ queryKey: hubPackKey });
    void client.invalidateQueries({ queryKey: hubSearchKey });
    void client.invalidateQueries({ queryKey: beatmapHubRecommendationsKey });
  } });
  const busy = submit.isPending || archive.isPending;
  const dirty = Boolean(folderId || title !== (edit?.title ?? "") || description !== (edit?.description ?? "") || isPrivate !== (edit?.is_private ?? false));
  const close = () => { if (busy) return; if (!published && dirty) setConfirmDiscard(true); else onClose(); };
  return <>
    <AppDialog overlayProps={{ className: "hub-overlay" }} open={visible} onOpenChange={(value) => { if (!value) close(); }} title={edit ? "更新曲包" : "发布曲包"} description={edit ? "选择本地收藏夹替换曲包内容，分享码保持不变。" : "分享你的练习清单。只上传谱面集清单，不上传本地文件。"} contentClassName="hub-dialog" closeDisabled={busy} footer={published ? <Button onClick={() => onOpenPack(published.id)}>查看曲包</Button> : <><Button disabled={busy} onClick={close}>取消</Button><Button form="hub-publish-form" type="submit" variant="primary" loading={submit.isPending} disabled={archive.isPending || !selected || !title.trim() || selected.beatmapset_count < 1 || selected.beatmapset_count > 500}>{connected ? edit ? "确认替换并更新" : "发布并复制分享码" : "连接身份后继续"}</Button></>}>
      {published ? <div className="hub-form hub-published"><CheckCircle2 size={38} /><h3>{edit ? "曲包已更新" : "曲包已发布"}</h3><label>分享码<input className="opp-input font-mono" readOnly value={`BPH-${published.id}`} /></label>{published.included != null ? <p>包含 {published.included} 个谱面集{published.skipped ? `，跳过 ${published.skipped} 个无 ID 条目` : ""}。</p> : null}<Button onClick={() => void copyHubCode(`BPH-${published.id}`).then((ok) => setNotice(ok ? "分享码已复制。" : "请选中分享码手动复制。"))}><Copy size={16} />复制分享码</Button></div> : <form id="hub-publish-form" className="hub-form" onSubmit={(event) => { event.preventDefault(); if (selected && title.trim() && !busy) submit.mutate(); }}>
        <fieldset disabled={busy} className="hub-form">
          <div className="hub-inline-actions"><h3>{edit ? "替换内容来源" : "选择内容"}</h3><Button type="button" size="sm" loading={archive.isPending} onClick={() => archive.mutate()}><FileInput size={15} />导入 .osz / .zip</Button></div>
          <label>本地收藏夹<select className="opp-input" value={folderId} required onChange={(event) => { setFolderId(event.target.value); if (!edit) setTitle(folders.find((folder) => folder.id === event.target.value)?.name ?? ""); }}><option value="">{collections.isPending ? "正在读取收藏夹…" : "选择收藏夹"}</option>{folders.map((folder) => <option value={folder.id} key={folder.id}>{folder.name}（{folder.beatmapset_count} 个谱面集）</option>)}</select></label>
          {collections.error ? <div role="alert" className="hub-error">收藏夹加载失败<Button type="button" onClick={() => void collections.refetch()}>重试</Button></div> : !collections.isPending && !folders.length ? <p className="hub-muted">没有可发布的收藏夹。可以先导入压缩包，或在收藏夹页创建；Lazer 和只读收藏夹不支持发布。</p> : null}
          {selected && (selected.beatmapset_count < 1 || selected.beatmapset_count > 500) ? <p className="hub-error">请选择包含 1–500 个谱面集的收藏夹。</p> : null}
          <label>曲包标题<input className="opp-input" maxLength={120} required value={title} onChange={(event) => setTitle(event.target.value)} /></label>
          <label><span>说明 <span className="hub-muted">（可选）</span></span><textarea className="opp-input" rows={3} maxLength={2000} value={description} onChange={(event) => setDescription(event.target.value)} placeholder="适合什么练习？有哪些值得推荐的谱面？" /></label>
          <fieldset className="hub-visibility"><legend>谁可以发现这个曲包</legend><label><input name="hub-visibility" type="radio" checked={!isPrivate} onChange={() => setIsPrivate(false)} /><span><strong>公开到社区</strong><small>其他玩家可以搜索和发现</small></span></label><label><input name="hub-visibility" type="radio" checked={isPrivate} onChange={() => setIsPrivate(true)} /><span><strong>仅凭分享码访问</strong><small>不会出现在社区推荐和搜索中</small></span></label></fieldset>
        </fieldset>
      </form>}
      {submit.error || archive.error ? <p className="hub-error" role="alert">{hubError(submit.error ?? archive.error)}</p> : null}
      {notice ? <p className="hub-feedback" role="status">{notice}</p> : null}
    </AppDialog>
    <AppDialog overlayProps={{ className: "hub-overlay" }} open={confirmDiscard} onOpenChange={setConfirmDiscard} title="放弃本次编辑？" description="尚未发布的表单内容将被清空。已导入的本地收藏夹会保留。" contentClassName="hub-dialog" size="sm" footer={<><Button onClick={() => setConfirmDiscard(false)}>继续编辑</Button><Button variant="danger" onClick={() => { setConfirmDiscard(false); onClose(); }}>放弃编辑</Button></>}><p>你也可以继续编辑后再发布。</p></AppDialog>
  </>;
}
