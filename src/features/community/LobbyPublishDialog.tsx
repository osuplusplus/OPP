import { useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { AppDialog } from "../../shared/components/AppDialog";
import { Button } from "../../shared/components/ui";
import type { CommunityActivity, CommunityLobby, CommunityPlatform, Ruleset } from "../../shared/types/osu";
import { communityApi } from "./api";
import { activityLabels, communityError, draftInput, lobbyDraft, platformLabels, rulesetLabels } from "./model";

export function LobbyPublishDialog({ edit, connected, identity, mode, onConnect, onClose, onSaved }: { edit: CommunityLobby | null; connected: boolean; identity: { username?: string | null; user_id?: number | null }; mode: Ruleset; onConnect: () => void; onClose: () => void; onSaved: (item: CommunityLobby) => void }) {
  const [original] = useState(() => lobbyDraft(edit ?? undefined, mode));
  const [draft, setDraft] = useState(original);
  const [confirmDiscard, setConfirmDiscard] = useState(false); const [validation, setValidation] = useState("");
  const dirty = Object.entries(draft).some(([key, value]) => value !== original[key as keyof typeof original]);
  const submit = useMutation({ mutationFn: () => communityApi.save(draftInput(draft), edit?.id ?? null), onSuccess: onSaved });
  const needsReconnect = ["INVALID_TOKEN", "INVALID_SESSION", "DEVICE_REVOKED", "AUTH_REQUIRED"].includes((submit.error as { code?: string } | null)?.code ?? "");
  const close = () => { if (submit.isPending) return; if (dirty) setConfirmDiscard(true); else onClose(); };
  const update = <K extends keyof typeof draft>(key: K, value: typeof draft[K]) => { setDraft((previous) => ({ ...previous, [key]: value })); setValidation(""); };
  return <>
    <AppDialog open onOpenChange={(open) => { if (!open) close(); }} size="lg" title={edit ? "编辑约玩" : "发布约玩"} description="写下想怎么玩、希望一起玩的伙伴，以及活动时间。" closeDisabled={submit.isPending} footer={<><Button disabled={submit.isPending} onClick={close}>取消</Button><Button variant="primary" type="submit" form="community-publish" loading={submit.isPending}>{connected ? edit ? "保存修改" : "确认发布" : "连接社区身份后继续"}</Button></>}>
      <form id="community-publish" className="community-form" onSubmit={(event) => { event.preventDefault(); if (submit.isPending) return; try { draftInput(draft); } catch (error) { setValidation(communityError(error)); return; } if (!connected) { onConnect(); return; } submit.mutate(); }}>
        <p className="community-publisher">发布者：<strong>{edit?.osu_username ?? identity.username ?? "当前 osu! 用户"}</strong> · osu! ID {edit?.osu_user_id ?? identity.user_id ?? "—"}</p>
        <fieldset disabled={submit.isPending} className="community-form-fields">
          <label>标题<input className="opp-input" required maxLength={120} value={draft.title} onChange={(event) => update("title", event.target.value)} placeholder="例如：今晚 8 点来两个人一起打 MP" /></label>
          <div className="community-form-row"><label>活动类型<select className="opp-input" value={draft.activity_type} onChange={(event) => update("activity_type", event.target.value as CommunityActivity)}>{Object.entries(activityLabels).map(([value, text]) => <option key={value} value={value}>{text}</option>)}</select></label>
            <label>游戏模式<select className="opp-input" value={draft.ruleset} onChange={(event) => update("ruleset", event.target.value as Ruleset)}>{Object.entries(rulesetLabels).map(([value, text]) => <option key={value} value={value}>{text}</option>)}</select></label></div>
          <label>平台（可选）<select className="opp-input" value={draft.platform ?? ""} onChange={(event) => update("platform", event.target.value ? event.target.value as CommunityPlatform : null)}><option value="">未指定</option>{Object.entries(platformLabels).map(([value, text]) => <option key={value} value={value}>{text}</option>)}</select></label>
          <div className="community-form-row"><label>开始时间<input className="opp-input" type="datetime-local" required value={draft.starts_at} onChange={(event) => update("starts_at", event.target.value)} /></label><label>结束时间<input className="opp-input" type="datetime-local" required value={draft.ends_at} onChange={(event) => update("ends_at", event.target.value)} /></label></div>
          <p className="community-muted">时间按本地时区显示；结束后会自动从公开大厅归档。</p>
          <label>活动描述<textarea className="opp-input" required rows={6} maxLength={2000} value={draft.description} onChange={(event) => update("description", event.target.value)} placeholder="想打什么图、适合什么水平、组队要求、如何联系你……" /></label>
          <p className="community-muted">{draft.description.length}/2000 · 玩家将通过 osu! 联系你。</p>
        </fieldset>
        {validation || submit.error ? <p role="alert" className="community-error">{validation || communityError(submit.error)}</p> : null}
        {needsReconnect ? <Button type="button" onClick={onConnect}>重新连接社区身份</Button> : null}
      </form>
    </AppDialog>
    <AppDialog open={confirmDiscard} onOpenChange={setConfirmDiscard} title="放弃本次编辑？" description="未发布的内容将被清空。" size="sm" footer={<><Button onClick={() => setConfirmDiscard(false)}>继续编辑</Button><Button variant="danger" onClick={onClose}>放弃编辑</Button></>}><p>也可以返回继续填写。</p></AppDialog>
  </>;
}
