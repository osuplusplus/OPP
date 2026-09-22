import { useRef, useState } from "react";
import { Plus, Users } from "lucide-react";
import { PageHeader } from "../../shared/components/PageHeader";
import { AppDialog } from "../../shared/components/AppDialog";
import { Button } from "../../shared/components/ui";
import type { BeatmapHubPack } from "../../shared/types/osu";
import { useBeatmapHubAuth, useHubPreview } from "./api";
import { BeatmapHubPackDialog } from "./BeatmapHubPackDialog";
import { HubBrowser } from "./HubBrowser";
import { HubIdentityDialog } from "./HubIdentityDialog";
import { HubPublishDialog } from "./HubPublishDialog";
import { hubError, type CommentDraft } from "./model";
import { useHubImport } from "./useHubImport";
import "./beatmapHub.css";

export function BeatmapHubPage() {
  const auth = useBeatmapHubAuth();
  const [selection, setSelection] = useState<{ id: string; fallback?: string } | null>(null);
  const [identityOpen, setIdentityOpen] = useState(false);
  const [publisher, setPublisher] = useState<{ edit: BeatmapHubPack | null } | null>(null);
  const [fallbackQuery, setFallbackQuery] = useState<string | null>(null);
  const [drafts, setDrafts] = useState<Record<string, CommentDraft>>({});
  const connected = Boolean(auth.data?.connected);
  const identity = `${auth.data?.user_id ?? "guest"}:${connected}`;
  const preview = useHubPreview(selection?.id ?? "", identity);
  const imports = useHubImport();
  const opener = useRef<HTMLElement | null>(null);
  const selectionRef = useRef<string | null>(null);
  const openPack = (id: string, fallback?: string) => {
    if (!selectionRef.current) opener.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    selectionRef.current = id; setSelection({ id, fallback }); setFallbackQuery(null);
  };
  const closePack = () => { selectionRef.current = null; setSelection(null); };
  const restorePackFocus = (event: Event) => {
    event.preventDefault();
    if (!selectionRef.current) {
      const target = opener.current?.isConnected ? opener.current : document.querySelector<HTMLInputElement>(".hub-search input");
      target?.focus({ preventScroll: true });
    }
  };
  const current = preview.data;
  return <div className="beatmaphub-page">
    <PageHeader title="BeatmapHub" actions={<div className="hub-inline-actions"><Button data-page-guide-hub-identity="true" onClick={() => setIdentityOpen(true)} variant="ghost"><Users size={16} />{connected ? auth.data?.display_name ?? "我的身份" : "连接身份"}</Button><Button variant="primary" data-page-guide-hub-publish="true" onClick={() => setPublisher({ edit: null })}><Plus size={17} />发布曲包</Button></div>} />
    <HubBrowser onOpen={openPack} fallbackQuery={fallbackQuery} onClearFallback={() => setFallbackQuery(null)} />
    {Object.values(imports.tasks).length ? <aside className="hub-task-list" aria-label="本次导入任务">{Object.values(imports.tasks).map((task) => <button key={task.id} onClick={() => openPack(task.id)} className="hub-task"><span><strong>{task.title}</strong><span>{task.message}</span></span><span>{task.phase === "error" ? "查看并重试 →" : task.phase === "complete" ? "查看结果 →" : "进行中 →"}</span></button>)}</aside> : null}
    {selection && (!current || preview.error) ? <AppDialog open onOpenChange={(value) => { if (!value) closePack(); }} onCloseAutoFocus={restorePackFocus} overlayProps={{ className: "hub-overlay" }} title="打开曲包" description={`BPH-${selection.id}`} contentClassName="hub-dialog" footer={preview.error ? <><Button onClick={() => void preview.refetch()}>重试打开</Button>{selection.fallback ? <Button onClick={() => { setFallbackQuery(selection.fallback!); closePack(); }}>改为关键词搜索</Button> : null}</> : undefined}>{preview.error ? <p className="hub-error" role="alert">{hubError(preview.error)}</p> : <p role="status">正在读取曲包内容…</p>}</AppDialog> : null}
    {selection && current && !preview.error ? <BeatmapHubPackDialog key={current.pack.id} preview={current} open onClose={closePack} onCloseAutoFocus={restorePackFocus} connected={connected} userId={auth.data?.user_id} onConnect={() => setIdentityOpen(true)} onEdit={(edit) => setPublisher({ edit })} onDeleted={closePack} task={imports.tasks[current.pack.id]} importBusy={imports.busy} onImport={imports.run} draft={drafts[current.pack.id] ?? { content: "", editing: null }} onDraft={(draft) => setDrafts((all) => ({ ...all, [current.pack.id]: draft }))} /> : null}
    {publisher ? <HubPublishDialog key={publisher.edit?.id ?? "new"} open edit={publisher.edit} connected={connected} onConnect={() => setIdentityOpen(true)} onClose={() => setPublisher(null)} onOpenPack={(id) => { setPublisher(null); openPack(id); }} /> : null}
    <HubIdentityDialog open={identityOpen} onClose={() => setIdentityOpen(false)} onConnected={() => setIdentityOpen(false)} />
  </div>;
}
