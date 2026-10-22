import { useEffect, useState } from "react";
import { AlertTriangle, FolderOpen, HardDrive, Link2, Square } from "lucide-react";
import { ErrorPanel } from "../../../shared/components/ErrorPanel";
import { Button, Card, SectionTitle } from "../../../shared/components/ui";
import { byteSize } from "../../../shared/lib/format";
import { desktopApi } from "../../../shared/lib/tauri";
import type { LazerDedupeProgress, LazerDedupeResult } from "../../../shared/types/osu";

const lazerDedupePhaseLabels: Record<string, string> = {
  "scan-lazer": "扫描 lazer 文件存储",
  "scan-stable": "扫描 stable 目录",
  hash: "计算 SHA-256 并匹配",
  link: "创建硬链接",
};

export function LazerDedupeCard() {
  const [acknowledged, setAcknowledged] = useState(false);
  const [busy, setBusy] = useState<"preview" | "apply" | null>(null);
  const [progress, setProgress] = useState<LazerDedupeProgress | null>(null);
  const [result, setResult] = useState<LazerDedupeResult | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [error, setError] = useState<unknown>(null);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void desktopApi.onLazerDedupeProgress((value) => { if (!disposed) setProgress(value); }).then((dispose) => { if (disposed) dispose(); else unlisten = dispose; });
    return () => { disposed = true; unlisten?.(); };
  }, []);

  const run = async (dryRun: boolean) => {
    setBusy(dryRun ? "preview" : "apply");
    setError(null);
    setProgress(null);
    try {
      setResult(await desktopApi.dedupeLazerFiles(dryRun));
      if (!dryRun) setConfirming(false);
    } catch (caught) {
      setError(caught);
    } finally {
      setBusy(null);
      setProgress(null);
    }
  };

  return <Card className="p-6">
    <div className="flex items-start gap-4">
      <div className="grid size-11 shrink-0 place-items-center rounded-2xl border border-[var(--theme-primary-soft)] bg-[var(--theme-primary-muted)] text-[var(--theme-primary)]"><HardDrive className="size-5" /></div>
      <SectionTitle title="压缩 Lazer 空间" description="lazer 以内容 SHA-256 为文件名存储文件。按大小分组对比 stable 谱面目录（Songs）文件的哈希，把内容完全一致的 lazer 副本替换为指向 stable 的硬链接，不修改任何文件内容。" />
    </div>
    <div className="mt-5 rounded-xl border border-amber-300/20 bg-amber-300/[0.06] p-4">
      <p className="flex items-center gap-2 text-sm font-semibold text-amber-100"><AlertTriangle className="size-4 shrink-0" />使用前请阅读风险提示</p>
      <ul className="mt-2 space-y-1.5 text-xs leading-5 text-amber-100/80">
        <li>· 硬链接要求两端位于同一磁盘分区，且文件系统支持硬链接（NTFS / ext4 / btrfs 等；FAT32 不支持）。跨分区或不支持的文件会自动跳过</li>
        <li>· 硬链接共享同一份数据，只占用一份空间：osu! 更新资源均为整文件替换，正常使用不受影响；删除任一端文件也不影响另一端</li>
        <li>· 创建硬链接后如果需要修改 stable 的谱面文件，请使用 osu! 客户端进行编辑，请不要手动修改</li>
        <li>· 如果真的需要手动修改文件，请删除源文件后创建新的写入，这样会断开硬链接，否则会造成该文件在 lazer 哈希值错误而报错</li>
        <li>· 执行前请关闭 osu!</li>
      </ul>
      <label className="mt-3 flex cursor-pointer items-center gap-2 text-xs font-medium text-amber-100">
        <input checked={acknowledged} className="size-3.5 accent-[var(--theme-primary)]" onChange={(event) => setAcknowledged(event.target.checked)} type="checkbox" />
        我已了解上述风险，确认 lazer 与 stable 目录配置正确
      </label>
    </div>
    <div className="mt-5 flex flex-wrap items-center gap-3">
      <Button disabled={!acknowledged || busy !== null} loading={busy === "preview"} onClick={() => void run(true)}><FolderOpen className="size-4" />{result ? "重新扫描可释放空间" : "扫描可释放空间"}</Button>
      {busy ? <Button onClick={() => void desktopApi.cancelLazerDedupe()} variant="secondary"><Square className="size-4" />取消</Button> : null}
    </div>
    {busy && progress ? <div className="mt-4 rounded-xl border border-white/[0.08] bg-white/[0.03] p-4">
      <div className="flex items-center justify-between gap-3 text-xs text-slate-400">
        <span>{lazerDedupePhaseLabels[progress.phase] ?? progress.phase}</span>
        <span className="tabular-nums">{progress.total > 0 ? `${progress.processed.toLocaleString()} / ${progress.total.toLocaleString()}` : `${progress.processed.toLocaleString()} 个文件`}</span>
      </div>
      <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-white/[0.08]"><div className="h-full rounded-full bg-[var(--theme-primary)] transition-[width]" style={{ width: `${progress.percent}%` }} /></div>
    </div> : null}
    {result ? <div className="mt-5 space-y-3">
      <p className="truncate font-mono text-xs text-slate-400" title={result.lazer_files_root}>{result.lazer_files_root}{result.stable_roots.length ? ` ⟵ ${result.stable_roots.join("、")}` : ""}</p>
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <div className="rounded-xl border border-white/[0.08] bg-white/[0.03] p-4"><p className="text-xs text-slate-500">lazer 文件</p><p className="mt-1 text-lg font-semibold tabular-nums text-white">{result.lazer_file_count.toLocaleString()} · {byteSize(result.lazer_total_size)}</p></div>
        <div className="rounded-xl border border-white/[0.08] bg-white/[0.03] p-4"><p className="text-xs text-slate-500">已是硬链接</p><p className="mt-1 text-lg font-semibold tabular-nums text-slate-300">{result.already_linked_count.toLocaleString()} · {byteSize(result.already_linked_size)}</p></div>
        <div className="rounded-xl border border-white/[0.08] bg-white/[0.03] p-4"><p className="text-xs text-slate-500">参与哈希对比</p><p className="mt-1 text-lg font-semibold tabular-nums text-slate-300">{result.hashed_stable_count.toLocaleString()}</p></div>
        <div className="rounded-xl border border-[var(--theme-primary-soft)] bg-[var(--theme-primary-muted)]/40 p-4">
          <p className="text-xs text-slate-500">{result.dry_run ? "可释放" : "已释放"}</p>
          <p className="mt-1 text-lg font-semibold tabular-nums text-[var(--theme-primary-light)]">{result.dry_run ? `${result.candidate_count.toLocaleString()} 个 · ${byteSize(result.reclaimable_size)}` : `${result.linked_count.toLocaleString()} 个 · ${byteSize(result.linked_size)}`}</p>
        </div>
      </div>
      {result.cancelled ? <p className="text-xs text-amber-200">任务已取消，以上为部分结果。</p> : null}
      {result.skipped_cross_volume_count > 0 ? <p className="text-xs text-slate-400">{result.skipped_cross_volume_count.toLocaleString()} 个文件（{byteSize(result.skipped_cross_volume_size)}）与 stable 不在同一分区，已跳过。</p> : null}
      {result.failed_count > 0 ? <div className="rounded-xl border border-white/[0.08] bg-white/[0.03] p-4">
        <p className="text-xs text-slate-400">{result.failed_count.toLocaleString()} 个文件处理失败（仅展示前 {result.failed.length} 条）：</p>
        <div className="mt-2 max-h-32 space-y-1 overflow-y-auto">
          {result.failed.map((failure) => <p className="truncate font-mono text-xs text-slate-500" key={failure.path} title={`${failure.path}：${failure.message}`}>{failure.path} — {failure.message}</p>)}
        </div>
      </div> : null}
      {!result.dry_run ? <p className="text-xs text-emerald-200">完成：{result.linked_count.toLocaleString()} 个文件已替换为硬链接，实际释放约 {byteSize(result.linked_size)}。可启动 lazer 验证内容正常。</p> : null}
    </div> : null}
    {result?.dry_run && !busy && result.candidate_count > 0 ? (
      confirming ? <div className="mt-5 rounded-xl border border-rose-400/25 bg-rose-400/[0.07] p-4">
        <p className="text-sm leading-6 text-rose-100">即将把 <span className="font-semibold">{result.candidate_count.toLocaleString()}</span> 个 lazer 文件替换为指向 stable 的硬链接，预计释放 <span className="font-semibold">{byteSize(result.reclaimable_size)}</span>。请确认已关闭 osu!。</p>
        <div className="mt-3 flex flex-wrap gap-3">
          <Button loading={busy === "apply"} onClick={() => void run(false)} variant="danger"><Link2 className="size-4" />确认执行</Button>
          <Button disabled={busy !== null} onClick={() => setConfirming(false)} variant="secondary">取消</Button>
        </div>
      </div> : <div className="mt-5">
        <Button disabled={!acknowledged} onClick={() => setConfirming(true)} variant="primary"><Link2 className="size-4" />创建硬链接并释放空间</Button>
      </div>
    ) : null}
    {result?.dry_run && !busy && result.candidate_count === 0 ? <p className="mt-5 text-xs text-slate-400">没有发现可与 stable 合并的重复文件。</p> : null}
    {error ? <div className="mt-4"><ErrorPanel error={error} /></div> : null}
  </Card>;
}
