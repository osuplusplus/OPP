import * as Dialog from "@radix-ui/react-dialog";
import { AlertTriangle, CheckCircle2, Clock, MinusCircle, PlusCircle, X } from "lucide-react";
import type { ReactNode } from "react";
import { Button } from "../../shared/components/ui";
import type { CollectionRefreshItem, CollectionRefreshPreview } from "../../shared/types/osu";

/** 「读取 Stable / lazer」前的差异确认弹窗数据：目标客户端与只读预览结果。 */
export interface PendingCollectionRefresh {
  client: "stable" | "lazer";
  preview: CollectionRefreshPreview;
}

function RefreshItemRow({ item, tone, detail }: { item: CollectionRefreshItem; tone: string; detail: string }) {
  return (
    <div className="flex items-center gap-3 rounded-xl border border-white/[0.06] bg-black/10 px-3 py-2.5">
      <span className={`size-2 shrink-0 rounded-full ${tone}`} />
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm text-slate-200">{item.name}</p>
        <p className="truncate text-xs text-slate-500">{detail}</p>
      </div>
      {item.pending_overwrite ? (
        <span className="flex shrink-0 items-center gap-1 rounded-md bg-amber-400/10 px-2 py-1 text-xs text-amber-200">
          <AlertTriangle className="size-3" />将覆盖未写回修改
        </span>
      ) : null}
    </div>
  );
}

function RefreshCategory({ title, icon, items }: { title: string; icon: ReactNode; items: CollectionRefreshItem[] }) {
  if (!items.length) return null;
  return (
    <section>
      <h4 className="mb-2 flex items-center gap-1.5 text-xs font-medium text-slate-400">{icon}{title}（{items.length}）</h4>
      <div className="space-y-1.5">
        {items.map((item) => (
          <RefreshItemRow
            detail={
              item.entries_added || item.entries_removed
                ? `新增 ${item.entries_added} 个难度、移除 ${item.entries_removed} 个难度`
                : `${item.entry_count} 个难度`
            }
            item={item}
            key={`${title}:${item.name}`}
            tone={title === "删除" ? "bg-red-400" : title === "保留" ? "bg-amber-300" : title === "更新" ? "bg-cyan-300" : "bg-emerald-300"}
          />
        ))}
      </div>
    </section>
  );
}

/** 读取游戏收藏夹的差异确认：确认后仅覆盖 OPP 侧收藏夹，不修改游戏文件。 */
export function RefreshPreviewDialog({
  pending,
  busy,
  onCancel,
  onConfirm,
}: {
  pending: PendingCollectionRefresh | null;
  busy: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const clientLabel = pending?.client === "lazer" ? "osu!lazer" : "osu!stable";
  return (
    <Dialog.Root onOpenChange={(open) => !open && !busy && onCancel()} open={Boolean(pending)}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-[260] bg-black/70 backdrop-blur-sm" />
        <Dialog.Content className="fixed left-1/2 top-1/2 z-[270] flex max-h-[min(680px,calc(100vh-32px))] w-[min(560px,calc(100vw-32px))] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-2xl border border-cyan-300/20 bg-[#101724] shadow-2xl outline-none">
          {pending ? (
            <>
              <div className="flex items-start justify-between gap-4 border-b border-white/[0.08] p-6">
                <div>
                  <Dialog.Title className="text-lg font-semibold text-white">读取 {clientLabel} 收藏夹</Dialog.Title>
                  <Dialog.Description className="mt-1 text-sm text-slate-400">
                    游戏侧与 OPP 的收藏夹存在差异。确认后将以上方游戏侧为准覆盖 OPP，不会修改游戏内的收藏夹。
                  </Dialog.Description>
                </div>
                <Dialog.Close asChild>
                  <button aria-label="关闭" className="text-slate-500 hover:text-white" disabled={busy} type="button"><X className="size-5" /></button>
                </Dialog.Close>
              </div>
              <div className="grid grid-cols-4 gap-3 border-b border-white/[0.08] p-5 text-center text-sm">
                <span className="rounded-lg bg-black/15 p-3 text-emerald-200">{pending.preview.additions.length}<small className="mt-1 block text-slate-500">新增</small></span>
                <span className="rounded-lg bg-black/15 p-3 text-cyan-200">{pending.preview.updates.length}<small className="mt-1 block text-slate-500">更新</small></span>
                <span className="rounded-lg bg-black/15 p-3 text-red-200">{pending.preview.removals.length}<small className="mt-1 block text-slate-500">删除</small></span>
                <span className="rounded-lg bg-black/15 p-3 text-amber-200">{pending.preview.preserved.length}<small className="mt-1 block text-slate-500">保留</small></span>
              </div>
              <div className="min-h-0 flex-1 space-y-5 overflow-y-auto p-5">
                <RefreshCategory icon={<PlusCircle className="size-3.5" />} items={pending.preview.additions} title="新增" />
                <RefreshCategory icon={<CheckCircle2 className="size-3.5" />} items={pending.preview.updates} title="更新" />
                <RefreshCategory icon={<MinusCircle className="size-3.5" />} items={pending.preview.removals} title="删除" />
                <RefreshCategory icon={<Clock className="size-3.5" />} items={pending.preview.preserved} title="保留" />
                {pending.preview.preserved.length ? (
                  <p className="text-xs text-slate-500">「保留」收藏夹在游戏侧已不存在，但 OPP 中尚有未写回的修改，本次读取不会动它们。</p>
                ) : null}
              </div>
              <div className="flex justify-end gap-2 border-t border-white/[0.08] p-5">
                <Button disabled={busy} onClick={onCancel} variant="ghost">取消</Button>
                <Button loading={busy} onClick={onConfirm}>覆盖 OPP 收藏夹</Button>
              </div>
            </>
          ) : null}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
