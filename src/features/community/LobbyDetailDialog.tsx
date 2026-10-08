import { useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { Copy, ExternalLink, Pencil, Square, Trash2 } from "lucide-react";
import { AppDialog } from "../../shared/components/AppDialog";
import { Button } from "../../shared/components/ui";
import type { CommunityLobby } from "../../shared/types/osu";
import { communityApi, useLobby } from "./api";
import { CommunityFailure, DetailField } from "./components";
import { activityTime, activityLabels, communityError, copyCommunityText, platformLabels, rulesetLabels, statusLabels } from "./model";

export function LobbyDetailDialog({ id, ownerId, onClose, onEdit, onChanged, restoreFocus }: { id: string; ownerId?: string; onClose: () => void; onEdit: (item: CommunityLobby) => void; onChanged: () => Promise<unknown>; restoreFocus: (event: Event) => void }) {
  const detail = useLobby(id); const item = detail.data;
  const [notice, setNotice] = useState(""); const [confirmation, setConfirmation] = useState<"close" | "delete" | null>(null);
  const action = useMutation({ mutationFn: async (operation: "close" | "delete") => {
    if (operation === "close") return communityApi.close(id);
    await communityApi.delete(id); return null;
  }, onSuccess: async (result) => { setConfirmation(null); await onChanged(); if (result) { await detail.refetch(); setNotice("活动已提前结束。"); } else onClose(); } });
  const canManage = Boolean(item && ownerId && item.owner_user_id === ownerId);
  const ended = item?.status === "closed" || item?.status === "ended";
  return <>
    <AppDialog open onOpenChange={(open) => { if (!open && !action.isPending) onClose(); }} title={item?.title ?? "约玩详情"} description="找到搭子，约好时间，一起打图。" size="lg" closeDisabled={action.isPending} onCloseAutoFocus={restoreFocus} footer={item ? <div className="community-detail-actions"><Button onClick={() => void copyCommunityText(item.osu_user_id).then(setNotice)}><Copy size={15} />复制 osu! ID</Button><Button variant="primary" onClick={() => void communityApi.openExternal(`https://osu.ppy.sh/users/${item.osu_user_id}`).catch(() => setNotice("无法打开主页，请重试。"))}><ExternalLink size={15} />打开 osu! 主页</Button></div> : undefined}>
      {detail.isPending ? <p role="status">正在加载约玩详情…</p> : detail.error ? <CommunityFailure error={detail.error} retry={() => void detail.refetch()} /> : item ? <div className="community-detail">
        <div className="community-tags"><span>{activityLabels[item.activity_type]}</span><span>{rulesetLabels[item.ruleset]}</span>{item.platform ? <span>{platformLabels[item.platform]}</span> : null}<span className={`community-status status-${item.status}`}>{statusLabels[item.status]}</span></div>
        <DetailField label="发布者">{item.osu_username} · osu! ID {item.osu_user_id}</DetailField><DetailField label="活动时间">{activityTime(item.starts_at)} — {activityTime(item.ends_at)}</DetailField>
        <DetailField label="活动描述"><p className="community-prose">{item.description}</p></DetailField>
        <p className="community-muted">感兴趣的话，通过 osu! 联系发布者，约好一起玩。</p>
        {canManage ? <div className="community-owner-actions">{!ended ? <><Button size="sm" disabled={action.isPending} onClick={() => onEdit(item)}><Pencil size={14} />编辑</Button><Button size="sm" disabled={action.isPending} onClick={() => setConfirmation("close")}><Square size={14} />提前结束</Button></> : null}<Button variant="danger" size="sm" disabled={action.isPending} onClick={() => setConfirmation("delete")}><Trash2 size={14} />删除</Button></div> : null}
      </div> : null}
      {notice ? <p role="status" className="community-notice">{notice}</p> : null}
    </AppDialog>
    <AppDialog open={Boolean(confirmation)} onOpenChange={(open) => { if (!open && !action.isPending) setConfirmation(null); }} title={confirmation === "close" ? "提前结束活动？" : "删除这条约玩？"} description={confirmation === "close" ? "结束后将移出公开大厅，仍可在我的发布中查看。" : "删除后其他玩家将无法再查看。"} size="sm" closeDisabled={action.isPending} footer={<><Button disabled={action.isPending} onClick={() => setConfirmation(null)}>取消</Button><Button variant="danger" loading={action.isPending} onClick={() => { if (confirmation) action.mutate(confirmation); }}>{confirmation === "close" ? "确认结束" : "确认删除"}</Button></>}>
      <p>{confirmation === "close" ? "结束后不能重新开放，可以另外发布新的活动。" : "请确认要删除这条信息。"}</p>{action.error ? <p role="alert" className="community-error">{communityError(action.error)}</p> : null}
    </AppDialog>
  </>;
}
