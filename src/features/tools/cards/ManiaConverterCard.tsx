import { useState } from "react";
import { FolderOpen, RefreshCw } from "lucide-react";
import { ErrorPanel } from "../../../shared/components/ErrorPanel";
import { Badge, Button, Card, SectionTitle } from "../../../shared/components/ui";
import { desktopApi } from "../../../shared/lib/tauri";
import type { ManiaConversionItem } from "../../../shared/types/osu";

export function ManiaConverterCard() {
  const [busy, setBusy] = useState(false);
  const [items, setItems] = useState<ManiaConversionItem[]>([]);
  const [error, setError] = useState<unknown>(null);
  const convert = async () => { const paths = await desktopApi.chooseManiaBeatmaps(); if (!paths.length) return; setBusy(true); setError(null); try { setItems((await desktopApi.convertManiaBeatmaps(paths)).items); } catch (value) { setError(value); } finally { setBusy(false); } };
  return <Card className="p-6"><div className="flex items-start gap-4"><div className="grid size-11 shrink-0 place-items-center rounded-2xl border border-[var(--theme-primary-soft)] bg-[var(--theme-primary-muted)] text-[var(--theme-primary)]"><RefreshCw className="size-5" /></div><SectionTitle title="Malody 谱面转换" description="选择一个或多个 .mcz 文件，转换后的 .osz 将保存在原文件目录。已有同名输出会安全跳过。" /></div><div className="mt-5"><Button loading={busy} onClick={() => void convert()}><FolderOpen className="size-4" />选择 .mcz 文件并转换</Button></div>{error ? <div className="mt-4"><ErrorPanel error={error} /></div> : null}{items.length ? <div className="mt-5 space-y-2">{items.map((item) => <div className="rounded-xl border border-white/[0.09] bg-white/[0.035] px-4 py-3" key={item.input}><div className="flex items-center justify-between gap-3"><span className="min-w-0 truncate text-sm font-medium text-slate-100">{item.input}</span><Badge tone={item.status === "completed" ? "success" : item.status === "skipped" ? "warning" : "pink"}>{item.status === "completed" ? "已完成" : item.status === "skipped" ? "已跳过" : "失败"}</Badge></div><p className="mt-1 truncate text-xs text-slate-300">{item.output ?? item.message}</p></div>)}</div> : null}</Card>;
}
