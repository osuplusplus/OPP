import { useMemo, useState } from "react";
import { FolderPlus, Heart } from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import { Button } from "../../shared/components/ui";
import { AppDialog } from "../../shared/components/AppDialog";
import { desktopApi } from "../../shared/lib/tauri";
import type { CollectionCandidate, CommandError } from "../../shared/types/osu";
import { collectionsQueryKey, collectionSummariesKey, collectionEntriesKey, useCollectionSummaries } from "./api";

export default function CollectionAddDialogContent({ defaultCreator, candidates, onClose }: { defaultCreator: string; candidates: CollectionCandidate[]; onClose: () => void }) {
  const queryClient = useQueryClient();
  const open = true;
  const setOpen = (next: boolean) => { if (!next) onClose(); };
  const foldersQuery = useCollectionSummaries(open);
  const { data } = foldersQuery;
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [folderId, setFolderId] = useState<string | null>(null);
  const [newFolder, setNewFolder] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const initialFolderId = open ? data?.folders.find((item) => !item.read_only)?.id ?? "" : "";
  const selectedFolderId = folderId ?? initialFolderId;

  const writableFolders = useMemo(() => open ? data?.folders.filter((folder) => !folder.read_only) ?? [] : [], [data, open]);
  const submit = async () => {
    const picked = candidates.filter((_, index) => selected.has(index));
    if (!picked.length) { setError("请至少选择一个难度"); return; }
    setBusy(true); setError(null);
    try {
      let target = selectedFolderId;
      if (newFolder.trim()) target = (await desktopApi.createCollection(newFolder.trim(), defaultCreator)).id;
      if (!target) { setError("请选择或创建一个收藏夹"); return; }
      await desktopApi.addCollectionEntries(target, picked);
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: collectionSummariesKey }),
        queryClient.invalidateQueries({ queryKey: collectionEntriesKey(target) }),
        queryClient.invalidateQueries({ queryKey: collectionsQueryKey, exact: true }),
      ]);
      setOpen(false);
    } catch (caught) { setError((caught as CommandError).message ?? String(caught)); } finally { setBusy(false); }
  };

  return (
    <AppDialog
      closeDisabled={busy}
      description="选择要保存的难度和目标文件夹。"
      footer={<><Button disabled={busy} onClick={() => setOpen(false)} variant="ghost">取消</Button><Button disabled={!selected.size || (!selectedFolderId && !newFolder.trim())} loading={busy} onClick={() => void submit()}><FolderPlus className="size-4" />加入收藏夹</Button></>}
      icon={<Heart className="size-5 text-pink-300" />}
      onOpenChange={setOpen}
      open={open}
      title="加入收藏夹"
    >
      <div className="max-h-48 space-y-2 overflow-y-auto rounded-xl border border-white/[0.08] bg-black/15 p-3">{candidates.map((candidate, index) => <label className="flex cursor-pointer items-center gap-3 rounded-lg px-2 py-2 hover:bg-white/[0.05]" key={`${candidate.beatmap_id ?? candidate.checksum ?? index}-${candidate.difficulty_name}`}><input checked={selected.has(index)} className="size-4 accent-[var(--theme-primary)]" onChange={() => setSelected((current) => { const next = new Set(current); if (next.has(index)) next.delete(index); else next.add(index); return next; })} type="checkbox" /><span className="min-w-0"><strong className="block truncate text-sm text-slate-200">{candidate.title || "未命名谱面"} <span className="text-slate-500">[{candidate.difficulty_name}]</span></strong><small className="block truncate text-xs text-slate-500">{candidate.artist} · {candidate.creator}</small></span></label>)}</div>
      {foldersQuery.isError ? <Button onClick={() => void foldersQuery.refetch()} variant="ghost">读取收藏夹失败，点击重试</Button> : foldersQuery.isLoading ? <p role="status" className="mt-3 text-sm text-slate-400">正在读取收藏夹…</p> : null}
      <label className="mt-4 block text-xs text-slate-400">目标收藏夹<select className="mt-1.5 w-full rounded-xl border border-white/[0.1] bg-black/20 px-3 py-2.5 text-sm text-slate-200" disabled={Boolean(newFolder.trim())} onChange={(event) => setFolderId(event.target.value)} value={selectedFolderId}><option value="">选择已有收藏夹</option>{writableFolders.map((folder) => <option key={folder.id} value={folder.id}>{folder.name}</option>)}</select></label>
      <label className="mt-3 block text-xs text-slate-400">或新建收藏夹<input className="mt-1.5 w-full rounded-xl border border-white/[0.1] bg-black/20 px-3 py-2.5 text-sm text-slate-200" onChange={(event) => setNewFolder(event.target.value)} placeholder="例如：想练的流图" value={newFolder} /></label>
      {error ? <p className="mt-3 text-sm text-rose-200">{error}</p> : null}
    </AppDialog>
  );
}
