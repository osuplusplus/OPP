import { useEffect, useRef } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { desktopApi } from "../../shared/lib/tauri";
import type { CommandError } from "../../shared/types/osu";
import { collectionSummariesKey, collectionsQueryKey } from "./api";
import { updateCollectionTask } from "./taskStatus";

/** osu!stable 退出后自动补写回收藏夹（含启动自愈与游戏侧变更合并）。
 *
 * 「导入 Stable / 补齐谱面」在 osu! 运行时只完成下载与解压（写 collection.db
 * 会被游戏退出时的回写覆盖），这里监视 stable 进程退出并自动收尾：
 *
 * - 游戏退出时用启动时的内存回写 collection.db，会覆盖 OPP 先前的写入——
 *   因此检测到游戏侧变更（`game_changed`）时先「读取 Stable」把游戏当前
 *   收藏合并进 OPP（游戏内新增会被导入、删除会被尊重、OPP 自建收藏夹保留），
 *   再整体写回，保证最终文件同时包含游戏侧与 OPP 两侧的收藏夹。
 * - 仅 OPP 有待写回变更时直接写回。
 * - OPP 启动时若 stable 未运行，同样检查一次（自愈上次错过的写回）。
 * 写回失败除“游戏又开起来了”外都会以任务失败形式提示，不再静默。 */
export function StableWriteBackWatcher() {
  const client = useQueryClient();
  const stableWasRunning = useRef(false);
  const writing = useRef(false);
  useEffect(() => {
    let disposed = false;
    let timer: number | undefined;
    const disposeFns: (() => void)[] = [];
    const schedule = (delay: number) => {
      window.clearTimeout(timer);
      timer = window.setTimeout(() => void reconcile(), delay);
    };
    const reconcile = async () => {
      if (disposed || writing.current) return;
      writing.current = true;
      try {
        // 复位上次任务遗留的取消标志，自动补写不受历史取消影响。
        await desktopApi.beginCollectionTask();
        const status = await desktopApi.getCollectionSyncStatus();
        if (disposed) return;
        if (status.in_sync && !status.game_changed) return;
        // 游戏侧有变更（通常是退出时回写覆盖）：先合并游戏当前收藏再写回，
        // 否则直接写会把游戏内的增删覆盖回旧状态。
        if (status.game_changed) {
          await desktopApi.refreshCollections("stable");
          if (disposed) return;
        }
        const written = await desktopApi.writeStableCollections();
        if (disposed) return;
        updateCollectionTask({
          phase: "completed",
          message: status.game_changed
            ? `已合并游戏侧收藏夹变更并写回 ${written.written_folders} 个收藏夹到 collection.db（重启 osu! 后在游戏内生效）。`
            : `osu! 已退出，已自动写回 ${written.written_folders} 个收藏夹到 collection.db。`,
          processed: 1,
          total: 1,
        });
        await Promise.all([
          client.invalidateQueries({ queryKey: collectionsQueryKey }),
          client.invalidateQueries({ queryKey: collectionSummariesKey }),
          client.invalidateQueries({ queryKey: ["collection-entries"] }),
        ]);
      } catch (caught) {
        // 游戏又开起来了（GAME_RUNNING）：等下一次退出再试，不打扰用户。
        if ((caught as CommandError)?.code === "GAME_RUNNING" || disposed) return;
        updateCollectionTask({
          phase: "failed",
          message: "游戏退出后自动写回收藏夹失败，可到收藏夹页手动“补齐并写回 Stable”重试",
          errors: [(caught as CommandError)?.message ?? String(caught)],
        });
      } finally {
        writing.current = false;
      }
    };
    const handle = (status: { clients: Array<{ client: string; running: boolean }> }) => {
      const running = status.clients.some((entry) => entry.client === "stable" && entry.running);
      // 退出后等几秒再写：给游戏落盘自身状态与进程完全退出的时间。
      if (stableWasRunning.current && !running) schedule(4_000);
      stableWasRunning.current = running;
    };
    void desktopApi.getGameStatus().then((status) => {
      handle(status);
      // 启动自愈：stable 未在运行时检查一次待写回/游戏侧变更。
      if (!disposed && !status.clients.some((entry) => entry.client === "stable" && entry.running)) {
        schedule(4_000);
      }
    }).catch(() => undefined);
    void desktopApi.onGameStatusChanged(handle)
      .then((unlisten) => { if (disposed) unlisten(); else disposeFns.push(unlisten); })
      .catch(() => undefined);
    return () => { disposed = true; window.clearTimeout(timer); disposeFns.forEach((dispose) => dispose()); };
  }, [client]);
  return null;
}
