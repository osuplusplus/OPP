import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Button } from "../../shared/components/ui";
import { hubApi, hubKey, hubPackKey, useHubComments } from "./api";
import { hubError, type CommentDraft } from "./model";

export function HubComments({ id, userId, canEdit, connected, onConnect, draft, onDraft }: {
  id: string; userId?: string | null; canEdit: boolean; connected: boolean; onConnect: () => void;
  draft: CommentDraft; onDraft: (draft: CommentDraft) => void;
}) {
  const client = useQueryClient();
  const comments = useHubComments(id);
  const mutation = useMutation({ mutationFn: async (removeId?: string) => {
    if (!connected) { onConnect(); return; }
    if (removeId) {
      await hubApi.deleteComment(removeId);
      if (draft.editing === removeId) onDraft({ content: "", editing: null });
    } else {
      if (draft.editing) await hubApi.updateComment(draft.editing, draft.content.trim());
      else await hubApi.createComment(id, draft.content.trim());
      onDraft({ content: "", editing: null });
    }
    void client.invalidateQueries({ queryKey: [...hubKey, "comments", id] });
    void client.invalidateQueries({ queryKey: [...hubPackKey, id] });
  } });
  return <section className="hub-comments" aria-label="曲包评论"><h3>讨论 <span className="hub-muted">{comments.data?.length ?? ""}</span></h3>
    {comments.isPending ? <p role="status">正在加载评论…</p> : comments.error ? <div className="hub-error" role="alert">评论加载失败<Button size="sm" onClick={() => void comments.refetch()}>重试评论</Button></div> : !comments.data?.length ? <p className="hub-muted">还没有讨论，分享一下你的练习感受吧。</p> : comments.data.map((item) => <article className="hub-comment" key={item.id}><div className="hub-inline-actions"><strong>{item.user.display_name}</strong><time className="hub-muted">{new Date(item.created_at).toLocaleDateString()}</time></div><p>{item.content}</p><div className="hub-inline-actions">{connected && item.user.id === userId ? <Button size="sm" variant="ghost" disabled={mutation.isPending} onClick={() => onDraft({ content: item.content, editing: item.id })}>编辑评论</Button> : null}{connected && (canEdit || item.user.id === userId) ? <Button size="sm" variant="ghost" disabled={mutation.isPending} onClick={() => mutation.mutate(item.id)}>删除评论</Button> : null}</div></article>)}
    <form className="hub-form" onSubmit={(event) => { event.preventDefault(); if (draft.content.trim() && !mutation.isPending) mutation.mutate(undefined); }}>
      <label>{draft.editing ? "编辑评论" : "写评论"}<textarea className="opp-input" rows={2} maxLength={2000} value={draft.content} disabled={mutation.isPending} onChange={(event) => onDraft({ ...draft, content: event.target.value })} placeholder="分享你的练习感受…" /></label>
      <div className="hub-inline-actions">{draft.editing ? <Button type="button" disabled={mutation.isPending} onClick={() => onDraft({ content: "", editing: null })}>取消编辑</Button> : null}<Button type="submit" loading={mutation.isPending} disabled={!draft.content.trim()}>{connected ? draft.editing ? "保存评论" : "发送评论" : "连接身份后发送"}</Button></div>
    </form>
    {mutation.error ? <p role="alert" className="hub-error">{hubError(mutation.error)}</p> : null}
  </section>;
}
