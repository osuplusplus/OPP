import { useState } from "react";
import { desktopApi } from "../../shared/lib/tauri";
import { errorMessage } from "../../shared/lib/format";
import type { BeatmapDownloadResult, OsuClient } from "../../shared/types/osu";

/** Explicit OS file handoff, independent of collection database writes. */
export function DownloadResultActions({ result, showArchiveActions = true, showDestinationAction = true }: {
  result: BeatmapDownloadResult;
  showArchiveActions?: boolean;
  showDestinationAction?: boolean;
}) {
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
      {showArchiveActions && paths.length > 0 && <button disabled={opening} onClick={() => void openArchives()}>{opening ? "正在打开曲包…" : failedPaths.length ? `重试打开失败曲包（${failedPaths.length}）` : `一键打开已下载曲包（${paths.length}）`}</button>}
      {showDestinationAction && result.destination && <button onClick={() => open(result.destination)}>打开下载目录</button>}
    </div>
    {showArchiveActions && paths.length > 0 && <p>打开曲包会调用系统关联的游戏；不会写入游戏收藏数据库。</p>}
    {result.failures.length > 0 && <div>
      <p>官网补下载需在浏览器登录 osu!，下载文件由浏览器保存。</p>
      {result.failures.map((failure) => <button key={failure.beatmapset_id} onClick={() => open(`https://osu.ppy.sh/beatmapsets/${failure.beatmapset_id}`, true)}>去官网补下载：{failure.title}</button>)}
    </div>}
    {notice && <p role="status">{notice}</p>}
    {error && <p role="alert">{error}</p>}
  </div>;
}

/** Open completed archives through a specific osu! client. */
export function DownloadTargetActions({ result, onNotice }: {
  result: BeatmapDownloadResult;
  onNotice: (message: string) => void;
}) {
  const paths = [...new Set(result.completed_paths ?? [])];
  const [opening, setOpening] = useState<OsuClient | null>(null);
  const [openingDirectory, setOpeningDirectory] = useState(false);

  if (!paths.length && !result.destination) return null;

  const openWith = async (client: OsuClient) => {
    if (opening) return;
    setOpening(client);
    try {
      const outcome = await desktopApi.openBeatmapFiles(client, paths);
      const clientName = client === "lazer" ? "lazer" : "Stable";
      const failures = outcome.failures.length ? `：${outcome.failures.join("；")}` : "";
      onNotice(`已交给 ${clientName} 导入 ${outcome.opened} 个曲包${outcome.failed ? `，${outcome.failed} 个失败` : ""}${failures}。`);
    } catch (error) {
      onNotice(errorMessage(error));
    } finally {
      setOpening(null);
    }
  };

  const openDirectory = async () => {
    if (openingDirectory || !result.destination) return;
    setOpeningDirectory(true);
    try {
      await desktopApi.openDownloadedPath(result.destination);
      onNotice("已打开下载目录。");
    } catch (error) {
      onNotice(errorMessage(error));
    } finally {
      setOpeningDirectory(false);
    }
  };

  return <>
    {paths.length > 0 && <>
      <button disabled={opening !== null || openingDirectory} onClick={() => void openWith("stable")}>
        {opening === "stable" ? "正在导入 Stable…" : `一键导入 Stable（${paths.length}）`}
      </button>
      <button disabled={opening !== null || openingDirectory} onClick={() => void openWith("lazer")}>
        {opening === "lazer" ? "正在导入 lazer…" : `一键导入 lazer（${paths.length}）`}
      </button>
    </>}
    {result.destination && <button disabled={opening !== null || openingDirectory} onClick={() => void openDirectory()}>
      {openingDirectory ? "正在打开目录…" : "打开下载目录"}
    </button>}
  </>;
}
