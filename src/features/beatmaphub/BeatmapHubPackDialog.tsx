import * as Dialog from "@radix-ui/react-dialog";
import { useEffect, useState } from "react";
import { Copy, Download, Heart, LockKeyhole, MessageCircle, Pencil, Star, Trash2, X } from "lucide-react";
import { Badge, Button } from "../../shared/components/ui";
import { DifficultyIcon } from "../../shared/components/DifficultyIcon";
import { desktopApi } from "../../shared/lib/tauri";
import type { BeatmapHubComment, BeatmapHubPack, OnlineBeatmapset } from "../../shared/types/osu";

export function BeatmapHubPackDialog({
  busy, currentUserId, downloadMissing, onClose, onDelete = () => undefined, onDownloadMissingChange, onFavorite, onImport, onLike, onRate, onUpdate, pack, resolved, resolveProgress,
}: {
  busy: string | null;
  currentUserId: string | null | undefined;
  downloadMissing: boolean;
  onClose: () => void;
  onDelete?: () => void;
  onDownloadMissingChange: (value: boolean) => void;
  onFavorite: () => void;
  onLike: () => void;
  onImport: () => void;
  onRate: (score: number) => void;
  onUpdate: () => void;
  pack: BeatmapHubPack | null;
  resolved: OnlineBeatmapset[];
  resolveProgress: number;
}) {
  const [comments, setComments] = useState<BeatmapHubComment[]>([]);
  const [comment, setComment] = useState("");
  const [editing, setEditing] = useState<string | null>(null);
  const [commentBusy, setCommentBusy] = useState(false);
  const difficulties = resolved.flatMap((set) => set.beatmaps ?? []).filter((beatmap) => typeof beatmap.difficulty_rating === "number");
  const easiest = difficulties.reduce<(typeof difficulties)[number] | null>((value, beatmap) => !value || beatmap.difficulty_rating! < value.difficulty_rating! ? beatmap : value, null);
  const hardest = difficulties.reduce<(typeof difficulties)[number] | null>((value, beatmap) => !value || beatmap.difficulty_rating! > value.difficulty_rating! ? beatmap : value, null);
  const packId = pack?.id;
  useEffect(() => {
    if (!packId) return;
    const loadComments = desktopApi.getBeatmapHubComments;
    if (typeof loadComments !== "function") return;
    void loadComments(packId).then(setComments).catch(() => setComments([]));
  }, [packId]);
  const submitComment = async () => {
    if (!pack || !comment.trim()) return;
    setCommentBusy(true);
    try {
      const value = editing ? await desktopApi.updateBeatmapHubComment(editing, comment) : await desktopApi.createBeatmapHubComment(pack.id, comment);
      setComments((current) => editing ? current.map((item) => item.id === value.id ? value : item) : [...current, value]);
      setComment(""); setEditing(null);
    } finally { setCommentBusy(false); }
  };
  return <Dialog.Root onOpenChange={(open) => !open && onClose()} open={pack !== null}>
    <Dialog.Portal>
      <Dialog.Overlay className="fixed inset-0 z-[80] bg-black/55 backdrop-blur-md" />
      <Dialog.Content className="beatmap-detail-dialog fixed left-1/2 top-1/2 z-[90] max-h-[min(720px,calc(100vh-2rem))] w-[min(720px,calc(100vw-2rem))] -translate-x-1/2 -translate-y-1/2 overflow-y-auto outline-none">
        {pack ? <>
          <div className="relative overflow-hidden border-b border-white/[0.08] bg-[#080b14] p-7">
            <div className="absolute inset-0 bg-[linear-gradient(125deg,rgba(34,211,238,.13),transparent_52%),linear-gradient(35deg,rgba(168,85,247,.12),transparent_58%)]" />
            <div className="relative flex items-start justify-between gap-5">
              <div className="min-w-0"><div className="flex flex-wrap items-center gap-2"><Badge tone={pack.is_private ? "warning" : "success"}>BPH-{pack.id}</Badge><Badge>{pack.is_private ? <><LockKeyhole className="size-3" />私有</> : "公开"}</Badge><span className="text-xs text-slate-400">{pack.owner.display_name}</span></div><Dialog.Title className="mt-4 truncate text-2xl font-semibold text-white">{pack.title}</Dialog.Title><Dialog.Description className="mt-2 max-w-2xl whitespace-pre-wrap text-sm leading-6 text-slate-300">{pack.description || "暂无描述"}</Dialog.Description></div>
              <Button aria-label="关闭曲包详情" className="bg-black/35 backdrop-blur" onClick={onClose} size="icon"><X className="size-4" /></Button>
            </div>
            <div className="relative mt-5 flex flex-wrap items-center gap-4 text-xs text-slate-400"><span>{pack.beatmapset_ids.length} 个谱面集</span>{easiest && hardest ? <span className="inline-flex items-center gap-1.5"><DifficultyIcon mode={easiest.mode ?? "osu"} showValue stars={easiest.difficulty_rating} /><span>至</span><DifficultyIcon mode={hardest.mode ?? "osu"} showValue stars={hardest.difficulty_rating} /></span> : pack.stars_min != null && pack.stars_max != null ? <span className="inline-flex items-center gap-1"><Star aria-hidden="true" className="size-3.5 text-amber-300" />{pack.stars_min.toFixed(2)} - {pack.stars_max.toFixed(2)}★</span> : null}<span>{pack.rating.average?.toFixed(1) ?? "暂无"} 分 · {pack.rating.count} 人</span><span>点赞 {pack.likes?.count ?? 0}</span><span>评论 {pack.comments?.count ?? 0}</span><span>已解析 {resolved.length}/{pack.beatmapset_ids.length}{resolveProgress < pack.beatmapset_ids.length ? "…" : ""}</span></div>
          </div>
          <div className="grid gap-5 p-7 lg:grid-cols-[1fr_220px]">
            <div><h3 className="text-sm font-semibold text-white">曲包内容</h3><div className="mt-3 max-h-80 space-y-2 overflow-y-auto pr-2">{pack.beatmapset_ids.map((id, index) => { const set = resolved.find((value) => value.id === id); return <div className="flex items-center gap-3 border-y border-white/[0.06] px-3 py-3" key={id}><span className="w-6 text-right text-xs text-slate-600">{index + 1}</span><div className="min-w-0"><p className="truncate text-sm text-slate-200">{set?.title ?? `Beatmapset #${id}`}</p><p className="truncate text-xs text-slate-500">{set ? `${set.artist} · ${set.creator} · ${set.beatmaps?.length ?? 0} 难度` : "元数据暂不可用，将以占位条目导入"}</p></div></div>; })}</div></div>
            <div className="space-y-3 border-t border-white/[0.08] pt-5 lg:border-l lg:border-t-0 lg:pl-5 lg:pt-0"><label className="flex items-center gap-2 text-sm text-slate-300"><input checked={downloadMissing} className="accent-cyan-300" onChange={(event) => onDownloadMissingChange(event.target.checked)} type="checkbox" />同时下载缺失谱面</label><Button className="w-full" loading={busy === "import"} onClick={onImport}><Download className="size-4" />确认导入</Button><div className="flex gap-2"><Button className="flex-1" loading={busy === "favorite"} onClick={onFavorite} variant="secondary"><Heart className={`size-4 ${pack.viewer?.favorited ? "fill-current text-pink-300" : ""}`} />{pack.viewer?.favorited ? "已收藏" : "收藏"}</Button><Button className="flex-1" loading={busy === "like"} onClick={onLike} variant="secondary"><Heart className={`size-4 ${pack.viewer?.liked ? "fill-current text-cyan-300" : ""}`} />{pack.viewer?.liked ? "已点赞" : "点赞"}</Button></div><div className="flex justify-center gap-1">{[1, 2, 3, 4, 5].map((score) => <button aria-label={`${score} 星`} className="p-1 text-amber-300 disabled:opacity-50" disabled={busy === "rating"} key={score} onClick={() => onRate(score)} type="button"><Star className={`size-5 ${score <= (pack.viewer?.rating ?? 0) ? "fill-current" : ""}`} /></button>)}</div><Button className="w-full" onClick={() => void navigator.clipboard.writeText(`BPH-${pack.id}`)} variant="secondary"><Copy className="size-4" />复制分享码</Button>{pack.viewer?.can_edit ? <Button className="w-full" loading={busy === "update"} onClick={onUpdate} variant="ghost">用所选收藏夹更新</Button> : null}</div>
          </div>
          <div className="border-t border-white/[0.08] p-7"><h3 className="flex items-center gap-2 text-sm font-semibold text-white"><MessageCircle className="size-4" />评论 ({comments.length})</h3><div className="mt-3 space-y-3">{comments.map((item) => <div className="border-y border-white/[0.06] py-3" key={item.id}><div className="flex items-center justify-between gap-3"><span className="text-xs text-slate-400">{item.user.display_name}</span><span className="text-[11px] text-slate-600">{new Date(item.created_at).toLocaleString()}</span></div><p className="mt-1 whitespace-pre-wrap text-sm text-slate-300">{item.content}</p><div className="mt-2 flex gap-2">{currentUserId === item.user.id ? <Button onClick={() => { setEditing(item.id); setComment(item.content); }} size="sm" variant="ghost"><Pencil className="size-3" />编辑</Button> : null}{pack.viewer?.can_edit || currentUserId === item.user.id ? <Button onClick={() => void desktopApi.deleteBeatmapHubComment(item.id).then(() => setComments((current) => current.filter((value) => value.id !== item.id)))} size="sm" variant="ghost"><Trash2 className="size-3" />删除</Button> : null}</div></div>)}</div><div className="mt-4 flex gap-2"><input className="opp-input flex-1" maxLength={2000} onChange={(event) => setComment(event.target.value)} placeholder={editing ? "编辑评论" : "写评论"} value={comment} /><Button disabled={!comment.trim()} loading={commentBusy} onClick={() => void submitComment()}>{editing ? "保存" : "发送"}</Button></div></div>
        </> : null}
        {pack?.viewer?.can_edit ? <div className="border-t border-white/[0.08] px-7 pb-5"><Button loading={busy === "delete"} onClick={onDelete} size="sm" variant="ghost"><Trash2 className="size-4" />删除曲包</Button></div> : null}
      </Dialog.Content>
    </Dialog.Portal>
  </Dialog.Root>;
}
