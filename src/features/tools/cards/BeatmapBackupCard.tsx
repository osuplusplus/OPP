import { useEffect, useState } from "react";
import { CloudUpload, FolderDown, Info, RefreshCw, Trash2 } from "lucide-react";
import { Link } from "react-router-dom";
import { ErrorPanel } from "../../../shared/components/ErrorPanel";
import { Button, Card, SectionTitle } from "../../../shared/components/ui";
import { dateTime } from "../../../shared/lib/format";
import { desktopApi, normalizeError } from "../../../shared/lib/tauri";
import type { BeatmapHubBackup, BeatmapHubBackupRestoreResult } from "../../../shared/types/osu";

// 需要先连接 BeatmapHub 才能使用云端备份的会话错误码。
const AUTH_ERROR_CODES = new Set([
  "HUB_AUTH_REQUIRED",
  "AUTH_REQUIRED",
  "INVALID_TOKEN",
  "DEVICE_REVOKED",
  "IDENTITY_REQUIRED",
]);

export function BeatmapBackupCard() {
  const [busy, setBusy] = useState<"upload" | "refresh" | "delete" | "restore" | null>(null);
  const [backup, setBackup] = useState<BeatmapHubBackup | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [needAuth, setNeedAuth] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [restored, setRestored] = useState<BeatmapHubBackupRestoreResult | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [error, setError] = useState<unknown>(null);

  const applyLoaded = (value: BeatmapHubBackup | null) => {
    setBackup(value);
    setLoaded(true);
    setNeedAuth(false);
    setError(null);
  };

  const applyLoadError = (caught: unknown) => {
    const code = normalizeError(caught).code;
    if (code === "BACKUP_NOT_FOUND") {
      setBackup(null);
      setLoaded(true);
      setError(null);
    } else if (AUTH_ERROR_CODES.has(code)) {
      setNeedAuth(true);
    } else {
      setError(caught);
    }
  };

  // 挂载时读取云端备份状态；状态更新只发生在异步回调里。
  useEffect(() => {
    let disposed = false;
    desktopApi
      .getBeatmapBackup()
      .then((value) => { if (!disposed) applyLoaded(value); })
      .catch((caught) => { if (!disposed) applyLoadError(caught); });
    return () => { disposed = true; };
  }, []);

  const refresh = async () => {
    setBusy("refresh");
    try {
      applyLoaded(await desktopApi.getBeatmapBackup());
    } catch (caught) {
      applyLoadError(caught);
    } finally {
      setBusy(null);
    }
  };

  const upload = async () => {
    setBusy("upload");
    setError(null);
    setNotice(null);
    try {
      const result = await desktopApi.uploadBeatmapBackup();
      setNotice(`已备份 ${result.count.toLocaleString()} 个谱面集${result.skipped > 0 ? `（跳过重复 ${result.skipped.toLocaleString()} 个）` : ""}。`);
      await refresh();
    } catch (caught) {
      setError(caught);
    } finally {
      setBusy(null);
    }
  };

  const remove = async () => {
    setBusy("delete");
    setError(null);
    try {
      await desktopApi.deleteBeatmapBackup();
      setBackup(null);
      setConfirming(false);
      setRestored(null);
      setNotice("云端备份已删除。");
    } catch (caught) {
      setError(caught);
    } finally {
      setBusy(null);
    }
  };

  const restore = async () => {
    setBusy("restore");
    setError(null);
    setNotice(null);
    try {
      const result = await desktopApi.restoreBeatmapBackup();
      setRestored(result);
      setNotice(`已恢复到收藏夹「${result.folder_name}」。`);
    } catch (caught) {
      setError(caught);
    } finally {
      setBusy(null);
    }
  };

  return <Card className="p-6">
    <div className="flex items-start gap-4">
      <div className="grid size-11 shrink-0 place-items-center rounded-2xl border border-[var(--theme-primary-soft)] bg-[var(--theme-primary-muted)] text-[var(--theme-primary)]"><CloudUpload className="size-5" /></div>
      <SectionTitle title="谱面列表云备份" description="把本地谱面库（Stable 与 Lazer）的谱面集列表整体上传到 OPP 社区服务器保存，换机或重装后可在同一 BeatmapHub 账号下查看恢复。每个账号仅保留一份备份，重复上传为整体覆盖。" />
    </div>
    <div className="mt-5 flex items-start gap-2 rounded-xl border border-amber-300/15 bg-amber-300/[0.06] p-3 text-sm leading-5 text-amber-100"><Info className="mt-0.5 size-4 shrink-0" />备份内容为谱面集 ID 列表（不含谱面文件本身），上限 20000 个；本地自制的无在线 ID 谱面不会计入。上传前请先完成本地资源扫描。</div>
    {needAuth ? <div className="mt-5 rounded-xl border border-white/[0.08] bg-white/[0.03] p-4 text-sm leading-6 text-slate-300">使用云端备份需要先连接 BeatmapHub：请前往 <Link className="font-medium text-[var(--theme-primary-light)] underline underline-offset-2" to="/beatmaphub">曲包广场</Link> 完成连接，或在上传时使用已登录的 osu! 账号自动连接。</div> : (
      <>
        <div className="mt-5 flex flex-wrap items-center gap-3">
          <Button disabled={busy !== null} loading={busy === "upload"} onClick={() => void upload()}><CloudUpload className="size-4" />{backup ? "重新上传覆盖备份" : "上传本地谱面列表"}</Button>
          {backup ? <Button disabled={busy !== null} loading={busy === "restore"} onClick={() => void restore()}><FolderDown className="size-4" />恢复为收藏夹</Button> : null}
          <Button disabled={busy !== null} loading={busy === "refresh"} onClick={() => void refresh()} variant="secondary"><RefreshCw className="size-4" />刷新云端状态</Button>
        </div>
        {notice ? <p className="mt-4 text-sm text-emerald-200">{notice}</p> : null}
        {loaded && !backup && !needAuth ? <p className="mt-5 text-xs text-slate-400">云端还没有备份。点击「上传本地谱面列表」保存当前谱面库快照。</p> : null}
        {backup ? <div className="mt-5 space-y-3">
          <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
            <div className="rounded-xl border border-white/[0.08] bg-white/[0.03] p-4"><p className="text-xs text-slate-500">云端谱面集</p><p className="mt-1 text-lg font-semibold tabular-nums text-white">{backup.count.toLocaleString()}</p></div>
            <div className="rounded-xl border border-white/[0.08] bg-white/[0.03] p-4"><p className="text-xs text-slate-500">本地已拥有</p><p className="mt-1 text-lg font-semibold tabular-nums text-slate-300">{backup.locally_available_count.toLocaleString()}</p></div>
            <div className="rounded-xl border border-white/[0.08] bg-white/[0.03] p-4"><p className="text-xs text-slate-500">本地缺失</p><p className="mt-1 text-lg font-semibold tabular-nums text-[var(--theme-primary-light)]">{backup.missing_count.toLocaleString()}</p></div>
            <div className="rounded-xl border border-white/[0.08] bg-white/[0.03] p-4"><p className="text-xs text-slate-500">最近备份时间</p><p className="mt-1 text-sm font-semibold tabular-nums text-slate-300">{dateTime(backup.updated_at)}</p><p className="mt-0.5 text-[11px] text-slate-500">首次备份：{dateTime(backup.created_at)}</p></div>
          </div>
          {backup.missing_count > 0 ? <p className="text-xs text-slate-400">备份中有 {backup.missing_count.toLocaleString()} 个谱面集当前不在本地，可用于换机后的找回参考。</p> : <p className="text-xs text-emerald-200/80">本地已包含备份中的全部谱面集。</p>}
          {restored ? <div className="rounded-xl border border-emerald-300/20 bg-emerald-300/[0.06] p-4">
            <p className="text-sm leading-6 text-emerald-100">已恢复到收藏夹「<span className="font-semibold">{restored.folder_name}</span>」：{restored.total_sets.toLocaleString()} 个谱面集（本地已有 {restored.local_sets.toLocaleString()}、缺失 {restored.missing_sets.toLocaleString()}），共 {restored.entries.toLocaleString()} 条收藏条目。</p>
            <p className="mt-1 text-xs leading-5 text-emerald-100/70">本地已有的谱面集按难度完整导入；缺失的谱面集以占位条目导入，可在收藏页批量补齐下载。每次恢复都会新建一个收藏夹。</p>
            <Link className="mt-2 inline-flex items-center gap-1 text-sm font-medium text-[var(--theme-primary-light)] underline underline-offset-2" to="/collections">去收藏夹查看并补齐缺失谱面</Link>
          </div> : null}
          {confirming ? <div className="rounded-xl border border-rose-400/25 bg-rose-400/[0.07] p-4">
            <p className="text-sm leading-6 text-rose-100">确定删除云端备份吗？删除后需要重新上传才能恢复，此操作不可撤销。</p>
            <div className="mt-3 flex flex-wrap gap-3">
              <Button loading={busy === "delete"} onClick={() => void remove()} variant="danger"><Trash2 className="size-4" />确认删除</Button>
              <Button disabled={busy !== null} onClick={() => setConfirming(false)} variant="secondary">取消</Button>
            </div>
          </div> : <Button disabled={busy !== null} onClick={() => setConfirming(true)} variant="secondary"><Trash2 className="size-4" />删除云端备份</Button>}
        </div> : null}
      </>
    )}
    {error ? <div className="mt-4"><ErrorPanel error={error} onRetry={() => void refresh()} /></div> : null}
  </Card>;
}
