import { useState } from "react";
import { Database, FolderOpen } from "lucide-react";
import { ErrorPanel } from "../../../shared/components/ErrorPanel";
import { Button, Card, SectionTitle } from "../../../shared/components/ui";
import { byteSize } from "../../../shared/lib/format";
import { desktopApi } from "../../../shared/lib/tauri";
import type { LazerDiskUsage } from "../../../shared/types/osu";

export function LazerDiskUsageCard() {
  const [data, setData] = useState<LazerDiskUsage | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const scan = async () => { setBusy(true); setError(null); try { setData(await desktopApi.getLazerDiskUsage()); } catch (value) { setError(value); } finally { setBusy(false); } };
  return <Card className="p-6"><div className="flex items-start gap-4"><div className="grid size-11 shrink-0 place-items-center rounded-2xl border border-[var(--theme-primary-soft)] bg-[var(--theme-primary-muted)] text-[var(--theme-primary)]"><Database className="size-5" /></div><SectionTitle title="osu!lazer 占用统计" description="按 storage.ini 的 FullPath 定位数据目录，统计含硬链接的总大小与排除硬链接后的实际占用。" /></div><div className="mt-5"><Button loading={busy} onClick={() => void scan()}><FolderOpen className="size-4" />{data ? "重新统计" : "开始统计"}</Button></div>{data ? <div className="mt-5 space-y-3"><p className="truncate font-mono text-xs text-slate-400">{data.path}</p><div className="grid gap-3 sm:grid-cols-3"><div className="rounded-xl border border-white/[0.08] bg-white/[0.03] p-4"><p className="text-xs text-slate-500">总大小</p><p className="mt-1 text-lg font-semibold tabular-nums text-white">{byteSize(data.total_size)}</p></div><div className="rounded-xl border border-white/[0.08] bg-white/[0.03] p-4"><p className="text-xs text-slate-500">实际占用</p><p className="mt-1 text-lg font-semibold tabular-nums text-[var(--theme-primary-light)]">{byteSize(data.unique_size)}</p></div><div className="rounded-xl border border-white/[0.08] bg-white/[0.03] p-4"><p className="text-xs text-slate-500">文件数</p><p className="mt-1 text-lg font-semibold tabular-nums text-white">{data.file_count.toLocaleString()}</p></div></div></div> : null}{error ? <div className="mt-4"><ErrorPanel error={error} /></div> : null}</Card>;
}
