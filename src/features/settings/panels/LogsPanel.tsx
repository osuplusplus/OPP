import { useState, useEffect } from "react";
import { FolderOpen, ExternalLink, RefreshCw } from "lucide-react";
import { Card, SectionTitle, Button } from "../../../shared/components/ui";
import { desktopApi } from "../../../shared/lib/tauri";
import type { LogFileInfo } from "../../../shared/types/osu";

export function LogsPanel() {
  const [logDirectory, setLogDirectory] = useState<string | null>(null);
  const [logFiles, setLogFiles] = useState<LogFileInfo[]>([]);
  const [logError, setLogError] = useState<string | null>(null);

  const refreshLogs = async () => {
    try {
      setLogError(null);
      const [dir, files] = await Promise.all([
        desktopApi.getLogDirectory(),
        desktopApi.listLogFiles(),
      ]);
      setLogDirectory(dir);
      setLogFiles(files);
    } catch (error) {
      setLogError(String(error));
    }
  };

  useEffect(() => {
    queueMicrotask(() => {
      void refreshLogs();
    });
  }, []);

  return (
    <div className="space-y-5">
      <div>
        <h2 className="text-2xl font-bold text-white">日志与诊断</h2>
        <p className="mt-1 text-sm text-slate-400">
          OPP 会保留最近 5 次运行日志，内容已自动隐藏凭据和敏感参数。
        </p>
      </div>

      <Card className="p-6">
        <SectionTitle
          title="日志文件"
          description="查看应用运行日志，帮助排查问题。"
        />

        {logDirectory ? (
          <p className="mt-4 break-all font-mono text-xs text-slate-400">
            {logDirectory}
          </p>
        ) : null}

        {logError ? (
          <p className="mt-3 text-sm text-rose-200">{logError}</p>
        ) : null}

        <div className="mt-4 flex flex-wrap gap-2">
          <Button
            onClick={() => void desktopApi.openLogDirectory()}
            size="sm"
            variant="secondary"
          >
            <FolderOpen className="size-4" />
            打开日志文件夹
          </Button>

          {logFiles[0] ? (
            <Button
              onClick={() => void desktopApi.openLogFile(logFiles[0].name)}
              size="sm"
              variant="secondary"
            >
              <ExternalLink className="size-4" />
              打开当前日志
            </Button>
          ) : null}

          <Button onClick={() => void refreshLogs()} size="sm" variant="ghost">
            <RefreshCw className="size-4" />
            刷新
          </Button>
        </div>

        <div className="mt-4 space-y-2">
          {logFiles.map((file) => (
            <div
              className="flex items-center justify-between gap-3 rounded-lg border border-white/[0.06] px-3 py-2 text-xs"
              key={file.name}
            >
              <button
                className="truncate text-left text-slate-300 hover:text-white"
                onClick={() => void desktopApi.openLogFile(file.name)}
                type="button"
              >
                {file.name}
              </button>
              <span className="shrink-0 text-slate-500">
                {(file.size_bytes / 1024).toFixed(1)} KB
              </span>
            </div>
          ))}
        </div>
      </Card>
    </div>
  );
}
