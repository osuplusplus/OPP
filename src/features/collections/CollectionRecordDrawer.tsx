import * as Dialog from "@radix-ui/react-dialog";
import { useCallback, useEffect, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Plus, X } from "lucide-react";
import { desktopApi } from "../../shared/lib/tauri";
import { errorMessage } from "../../shared/lib/format";
import type { CollectionBrowseRow, CollectionPersonalRecord, LocalScoreRecord } from "../../shared/types/osu";
import { collectionBrowserKey } from "./api";
import { readSession, saveSession, scoreLabel } from "./browserModel";

export function CollectionRecordDrawer({ row, player, onClose }: { row: CollectionBrowseRow; player: string | null; onClose: () => void }) {
  const client = useQueryClient();
  const draftKey = `opp:collection-draft:${row.folder_id}:${row.entry.id}`;
  const [record, setRecord] = useState(() => readSession<CollectionPersonalRecord | null>(draftKey, null) ?? row.record);
  const current = useRef(record);
  const revision = useRef(record.revision);
  const saved = useRef(JSON.stringify(row.record));
  const flight = useRef<Promise<boolean> | null>(null);
  const [status, setStatus] = useState("");
  const [tagName, setTagName] = useState("");
  const [tagColor, setTagColor] = useState("#67e8f9");
  const [editing, setEditing] = useState<LocalScoreRecord | null>(null);
  const scores = useQuery({ queryKey: ["collection-scores", row.folder_id, row.entry.id], queryFn: () => desktopApi.getCollectionEntryScores(row.folder_id, row.entry.id, null), staleTime: 15_000 });
  const change = (next: CollectionPersonalRecord) => { current.current = next; setRecord(next); saveSession(draftKey, next); setStatus("等待保存…"); };
  const flush = useCallback((): Promise<boolean> => {
    if (flight.current) return flight.current;
    const work = async () => {
      while (JSON.stringify(current.current) !== saved.current) {
        const pending = current.current;
        setStatus("正在保存…");
        try {
          const result = await desktopApi.saveCollectionRecord(row.folder_id, row.entry.id, { ...pending, revision: revision.current });
          revision.current = result.revision;
          if (current.current === pending) {
            current.current = result; setRecord(result); saved.current = JSON.stringify(result); saveSession(draftKey, null);
          } else { saved.current = JSON.stringify(result); saveSession(draftKey, { ...current.current, revision: result.revision }); }
          await client.invalidateQueries({ queryKey: collectionBrowserKey });
          setStatus("已保存到 OPP");
        } catch (error) { setStatus(`保存失败：${errorMessage(error)}`); return false; }
      }
      return true;
    };
    const promise = work().finally(() => { flight.current = null; });
    flight.current = promise; return promise;
  }, [client, draftKey, row.entry.id, row.folder_id]);
  useEffect(() => { const timer = setTimeout(() => { void flush(); }, 600); return () => clearTimeout(timer); }, [record, flush]);
  useEffect(() => () => { void flush(); }, [flush]);
  const close = async () => { if (await flush()) onClose(); };
  const reload = async () => {
    try {
      await flight.current;
      const latest = await desktopApi.getCollectionRecord(row.folder_id, row.entry.id);
      current.current = latest; saved.current = JSON.stringify(latest); revision.current = latest.revision;
      setRecord(latest); setEditing(null); saveSession(draftKey, null); setStatus("已重新载入保存的记录");
    } catch (error) { setStatus(`保存失败：${errorMessage(error)}`); }
  };
  const addScore = () => setEditing({ id: crypto.randomUUID(), source: "manual", player: player ?? "", ruleset: row.entry.ruleset ?? "osu", scoring: "manual", beatmap_hash: row.entry.checksum ?? "", score: 0, accuracy: null, combo: null, mods: "", played_at: new Date().toISOString(), note: "" });
  const allScores = [...record.scores, ...(scores.data ?? []).filter((s) => !!player && s.player.toLowerCase() === player.toLowerCase())].sort((a, b) => (b.played_at ? Date.parse(b.played_at) : 0) - (a.played_at ? Date.parse(a.played_at) : 0));
  const missingRepresentative = !scores.isPending && record.representative && ![...record.scores, ...(scores.data ?? [])].some((s) => s.id === record.representative!.id);
  return <Dialog.Root open onOpenChange={(open) => { if (!open) void close(); }}><Dialog.Portal>
    <Dialog.Overlay className="collection-dialog-overlay" />
    <Dialog.Content className="collection-record-drawer">
      <div className="collection-drawer-heading"><div><small>{row.folder_name} · {row.slot || "个人记录"}</small><Dialog.Title>{row.entry.title}</Dialog.Title><Dialog.Description>{row.entry.difficulty_name} · 记录仅保存在当前收藏夹</Dialog.Description></div><button aria-label="关闭记录" onClick={() => void close()}><X /></button></div>
      <div className="collection-drawer-content">
        <label className="collection-field">图位<input value={record.slot_override ?? row.source_slot} placeholder="例如 NM1、HD2" maxLength={64} onChange={(e) => change({ ...record, slot_override: e.target.value.toUpperCase() })} /></label>
        {record.slot_override !== null && <button className="collection-text-button" onClick={() => change({ ...record, slot_override: null })}>恢复{row.source_slot ? `比赛图位 ${row.source_slot}` : "默认图位"}</button>}
        {row.pool_comment && <blockquote className="collection-source-note"><small>比赛评论{row.selected_by ? ` · ${row.selected_by}` : ""}</small><p>{row.pool_comment}</p></blockquote>}
        <section><h3>我的标签</h3><div className="collection-tags">{record.tags.map((tag, i) => <button key={`${tag.name}:${i}`} style={{ color: tag.color, borderColor: tag.color }} onClick={() => change({ ...record, tags: record.tags.filter((_, n) => n !== i) })} title="移除标签">{tag.name}<X size={12} /></button>)}</div>
          <form className="collection-tag-form" onSubmit={(event) => { event.preventDefault(); if (tagName.trim() && record.tags.length < 32) { change({ ...record, tags: [...record.tags, { name: tagName.trim(), color: tagColor }] }); setTagName(""); } }}><input aria-label="标签名称" value={tagName} maxLength={30} placeholder="练习 / 能拿分 / 容易失误" onChange={(e) => setTagName(e.target.value)} /><input aria-label="标签颜色" type="color" value={tagColor} onChange={(e) => setTagColor(e.target.value)} /><button aria-label="添加标签" disabled={!tagName.trim()}><Plus size={16} /></button></form>
        </section>
        <label className="collection-field">我的笔记<textarea rows={6} maxLength={30000} placeholder="记下节奏、难点，或一句下次想提醒自己的话…" value={record.note} onChange={(e) => change({ ...record, note: e.target.value })} /></label>
        <div className="collection-save-status" role="status">{status || "修改后自动保存"}{status.startsWith("保存失败") && <span className="flex flex-wrap gap-3"><button onClick={() => void flush()}>重试保存</button><button onClick={onClose}>保留草稿并关闭</button><button onClick={() => void reload()}>丢弃草稿并重载</button></span>}</div>
        <section><div className="collection-section-title"><h3>成绩记录</h3><button onClick={addScore}><Plus size={15} />手动补录</button></div>
          {!player && <p className="collection-muted">在页面顶部选择玩家后查看本地历史。</p>}
          {scores.isError && <button onClick={() => void scores.refetch()}>读取本地成绩失败，重试</button>}
          {record.representative && <div className="collection-score representative"><strong>代表成绩 · {scoreLabel(record.representative)}</strong><small>{record.representative.source} · {record.representative.scoring}{missingRepresentative ? " · 来源已不可用（保留快照）" : ""}</small><button onClick={() => change({ ...record, representative: null })}>取消代表成绩</button></div>}
          {editing && <form className="collection-score-form" onSubmit={(event) => { event.preventDefault(); change({ ...record, scores: [...record.scores.filter((s) => s.id !== editing.id), editing], representative: record.representative?.id === editing.id ? editing : record.representative }); setEditing(null); }}>
            <label>分数<input aria-label="分数" required type="number" min={0} max={Number.MAX_SAFE_INTEGER} step={1} value={editing.score} onChange={(e) => setEditing({ ...editing, score: Number(e.target.value) })} /></label>
            <label>准确率 %<input aria-label="准确率" type="number" min={0} max={100} step="any" value={editing.accuracy === null ? "" : Number((editing.accuracy * 100).toFixed(6))} onChange={(e) => setEditing({ ...editing, accuracy: e.target.value === "" ? null : Number(e.target.value) / 100 })} /></label>
            <label>Combo<input type="number" min={0} max={4294967295} value={editing.combo ?? ""} onChange={(e) => setEditing({ ...editing, combo: e.target.value === "" ? null : Number(e.target.value) })} /></label>
            <label>Mods<input value={editing.mods} onChange={(e) => setEditing({ ...editing, mods: e.target.value })} /></label>
            <label>计分体系<select value={editing.scoring} onChange={(e) => setEditing({ ...editing, scoring: e.target.value })}><option value="manual">未指定</option><option value="stable">Stable</option><option value="score_v2">Score V2</option><option value="lazer">lazer</option></select></label>
            <label>日期<input type="datetime-local" value={editing.played_at ? new Date(new Date(editing.played_at).getTime() - new Date(editing.played_at).getTimezoneOffset() * 60000).toISOString().slice(0, 16) : ""} onChange={(e) => setEditing({ ...editing, played_at: e.target.value ? new Date(e.target.value).toISOString() : null })} /></label>
            <label className="collection-score-note">备注<input value={editing.note} maxLength={2000} onChange={(e) => setEditing({ ...editing, note: e.target.value })} /></label>
            <button type="button" onClick={() => setEditing(null)}>取消</button><button type="submit">保存成绩</button>
          </form>}
          {!allScores.length && !scores.isLoading && <p className="collection-muted">还没有成绩，可手动记录一次练习。</p>}
          {allScores.map((score) => <div className="collection-score" key={score.id}><strong>{scoreLabel(score)}</strong><small>{score.source} · {score.scoring} · {score.player || "手动"} · {score.mods || "NM"}{score.combo === null ? "" : ` · ${score.combo}x`} · {score.played_at ? new Date(score.played_at).toLocaleString() : "日期未填"}</small>{score.note && <p>{score.note}</p>}<div><button onClick={() => change({ ...record, representative: score })}>设为代表成绩</button>{score.source === "manual" && <><button onClick={() => setEditing(score)}>编辑</button><button onClick={() => change({ ...record, scores: record.scores.filter((s) => s.id !== score.id) })}>删除</button></>}</div></div>)}
        </section>
      </div>
    </Dialog.Content>
  </Dialog.Portal></Dialog.Root>;
}
