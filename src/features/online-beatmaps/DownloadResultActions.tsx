import { useState } from "react";
import { desktopApi } from "../../shared/lib/tauri";
import { errorMessage } from "../../shared/lib/format";
import type { BeatmapDownloadResult } from "../../shared/types/osu";

/** Explicit OS file handoff, independent of collection database writes. */
export function DownloadResultActions({ result }: { result: BeatmapDownloadResult }) {
  const paths = [...new Set(result.completed_paths ?? [])];
  const [opening, setOpening] = useState(false);
  const [failedPaths, setFailedPaths] = useState<string[]>([]);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const openArchives = async () => {
    if (opening) return;
    setOpening(true); setNotice(null); setError(null);
    const failed: string[] = []; const messages: string[] = [];
    const targets = failedPaths.length ? failedPaths : paths;
    for (const path of targets) {
      try { await desktopApi.openDownloadedPath(path); }
      catch (caught) { failed.push(path); messages.push(`${path.split(/[\\/]/).pop()}：${errorMessage(caught)}`); }
    }
    setFailedPaths(failed);
    setNotice(`已交给系统打开 ${targets.length - failed.length} 个曲包${failed.length ? `，${failed.length} 个打开失败` : ""}。`);
    if (messages.length) setError(messages.join("；"));
    setOpening(false);
  };
  const open = (target: string, external = false) => {
    setError(null);
    void (external ? desktopApi.openExternal(target) : desktopApi.openDownloadedPath(target)).catch((caught) => setError(errorMessage(caught)));
  };
  return <div className="download-result-actions">
    <div>
      {paths.length > 0 && <button disabled={opening} onClick={() => void openArchives()}>{opening ? "正在打开曲包…" : failedPaths.length ? `重试打开失败曲包（${failedPaths.length}）` : `一键打开已下载曲包（${paths.length}）`}</button>}
      {result.destination && <button onClick={() => open(result.destination)}>打开下载目录</button>}
    </div>
    {paths.length > 0 && <p>打开曲包会调用系统关联的游戏；不会写入游戏收藏数据库。</p>}
    {result.failures.length > 0 && <div>
      <p>官网补下载需在浏览器登录 osu!，下载文件由浏览器保存。</p>
      {result.failures.map((failure) => <button key={failure.beatmapset_id} onClick={() => open(`https://osu.ppy.sh/beatmapsets/${failure.beatmapset_id}`, true)}>去官网补下载：{failure.title}</button>)}
    </div>}
    {notice && <p role="status">{notice}</p>}
    {error && <p role="alert">{error}</p>}
  </div>;
}
