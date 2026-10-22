import { useEffect, useState } from "react";
import { ExternalLink, FolderOpen, RefreshCw } from "lucide-react";
import { Button, Card, SectionTitle } from "../../../shared/components/ui";
import { desktopApi } from "../../../shared/lib/tauri";
import type { LogFileInfo } from "../../../shared/types/osu";

export function LogsPanel({ className }: { className?: string }) {
  const [logDirectory, setLogDirectory] = useState<string | null>(null);
  const [logFiles, setLogFiles] = useState<LogFileInfo[]>([]);
  const [logError, setLogError] = useState<string | null>(null);
  const refreshLogs = async () => { try { const [directory, files] = await Promise.all([desktopApi.getLogDirectory(), desktopApi.listLogFiles()]); setLogDirectory(directory); setLogFiles(files); setLogError(null); } catch (error) { setLogError(error instanceof Error ? error.message : "无法读取日志"); } };
  useEffect(() => { queueMicrotask(() => { void refreshLogs(); }); }, []);

  return (
    <Card className={className}>
      <SectionTitle title="日志与诊断" description="OPP 会保留最近 5 次运行日志，内容已自动隐藏凭据和敏感参数。" />
      {logDirectory ? <p className="mt-4 break-all font-mono text-xs text-slate-400">{logDirectory}</p> : null}
      {logError ? <p className="mt-3 text-sm text-rose-200">{logError}</p> : null}
      <div className="mt-4 flex flex-wrap gap-2">
        <Button onClick={() => void desktopApi.openLogDirectory()} size="sm" variant="secondary"><FolderOpen className="size-4" />打开日志文件夹</Button>
        {logFiles[0] ? <Button onClick={() => void desktopApi.openLogFile(logFiles[0].name)} size="sm" variant="secondary"><ExternalLink className="size-4" />打开当前日志</Button> : null}
        <Button onClick={() => void refreshLogs()} size="sm" variant="ghost"><RefreshCw className="size-4" />刷新</Button>
      </div>
      <div className="mt-4 space-y-2">{logFiles.map((file) => <div className="flex items-center justify-between gap-3 rounded-lg border border-white/[.06] px-3 py-2 text-xs" key={file.name}><button className="truncate text-left text-slate-300 hover:text-white" onClick={() => void desktopApi.openLogFile(file.name)} type="button">{file.name}</button><span className="shrink-0 text-slate-500">{(file.size_bytes / 1024).toFixed(1)} KB</span></div>)}</div>
    </Card>
  );
}
