import type { BeatmapDownloadResult, CommandError } from "../../shared/types/osu";
import { desktopApi } from "../../shared/lib/tauri";
import { resolveDefaultDownloadProvider } from "../online-beatmaps/downloadProvider";
import { beginCollectionTask, throwIfCollectionTaskCancelled, updateCollectionTask } from "./taskStatus";

/** 写回 collection.db；遇到 COLLECTION_CONFLICT（通常是 osu! 退出时重写了
 * 文件导致指纹过期）时先合并游戏侧收藏再重试一次，与全局监视器的恢复语义
 * 一致，避免用户手动“刷新后重试”。其余错误原样抛出。 */
async function writeStableCollectionsWithRecovery(): Promise<number> {
  try {
    return (await desktopApi.writeStableCollections()).written_folders;
  } catch (caught) {
    if ((caught as CommandError)?.code !== "COLLECTION_CONFLICT") throw caught;
    await desktopApi.refreshCollections("stable");
    return (await desktopApi.writeStableCollections()).written_folders;
  }
}

/** 一键导入 Stable 的结果汇总，供各入口拼装完成文案。 */
export interface StableImportOutcome {
  /** 需要下载的谱面集总数；0 表示本地已齐，只走写回。 */
  missingItems: number;
  /** 下载统计（无需下载时为全零）。 */
  completed: number;
  skipped: number;
  failed: number;
  failures: BeatmapDownloadResult["failures"];
  /** 本次解压进 Songs 的曲包数。 */
  extractedSets: number;
  /** collection.db 是否因 osu! 运行而延期（StableWriteBackWatcher 会在退出后补写）。 */
  writeDeferred: boolean;
  /** 实际写回的收藏夹数；延期时为 null。 */
  writtenFolders: number | null;
}

/** 一键导入 Stable：启用同步 → 下载缺失曲包 → 解压进 Songs（install 内完成，
 * 对运行中的游戏安全）→ 写回 collection.db（游戏运行时自动延期）。
 * 收藏夹页「导入 Stable / 补齐并写回」、RomAI 图池弹窗与 BeatmapHub 导入共用。
 * 进度经全局 CollectionTaskToast 展示；`hooks.onNotice` 用于页面级提示。 */
export async function importFoldersToStable(folderIds: string[], hooks: {
  onNotice?: (message: string) => void;
  onDownloadActive?: (active: boolean) => void;
} = {}): Promise<StableImportOutcome> {
  const outcome: StableImportOutcome = {
    missingItems: 0, completed: 0, skipped: 0, failed: 0, failures: [],
    extractedSets: 0, writeDeferred: false, writtenFolders: null,
  };
  await desktopApi.beginCollectionTask();
  // 图池等 OPP 专属收藏夹默认不参与 Stable 同步，导入时显式开启。
  const summaries = await desktopApi.listCollectionSummaries();
  const stableSyncOff = new Set(summaries.folders.filter((folder) => folder.stable_sync === false).map((folder) => folder.id));
  for (const folderId of folderIds) {
    if (stableSyncOff.has(folderId)) await desktopApi.enableCollectionStableSync(folderId);
  }
  hooks.onNotice?.("正在检查收藏夹中的缺失谱面…");
  beginCollectionTask({ phase: "checking", message: "正在检查收藏夹中的缺失谱面和旧 MD5…", processed: 0, total: 0, errors: [] });
  const detectedItems = await desktopApi.getCollectionDownloadItems(folderIds);
  throwIfCollectionTaskCancelled();
  const items = [...new Map(detectedItems.map((item) => [item.beatmapset_id, item])).values()];
  outcome.missingItems = items.length;
  let archivePaths: string[] = [];
  if (items.length) {
    const stable = (await desktopApi.getLocalSources()).find((source) => source.client === "stable");
    if (!stable?.valid || !stable.install_root) {
      throw new Error("请先在设置中配置有效的 osu!stable 安装目录，才能将线上谱面自动下载到游戏。");
    }
    const settings = await desktopApi.getSettings();
    const root = stable.install_root.replace(/[\\/]+$/, "");
    const destination = settings.beatmap_download_directory || `${root}\\OPP Downloads`;
    hooks.onDownloadActive?.(true);
    hooks.onNotice?.(`准备下载 ${items.length} 个缺失曲包…`);
    updateCollectionTask({ phase: "downloading", message: `准备下载 ${items.length} 个缺失曲包…`, processed: 0, total: items.length });
    try {
      // 单次批量下载与单次安装各限 500 个：缺失很多（如备份恢复）时按批次顺序执行。
      const BATCH_LIMIT = 500;
      for (let offset = 0; offset < items.length; offset += BATCH_LIMIT) {
        const batchIndex = Math.floor(offset / BATCH_LIMIT);
        const batches = Math.ceil(items.length / BATCH_LIMIT);
        const download = await desktopApi.downloadOnlineBeatmapsets({
          destination,
          provider: resolveDefaultDownloadProvider(settings),
          overwrite: false,
          include_video: settings.include_video_in_beatmap_downloads,
          open_after_download: false,
          items: items.slice(offset, offset + BATCH_LIMIT),
        });
        throwIfCollectionTaskCancelled();
        outcome.completed += download.completed;
        outcome.skipped += download.skipped;
        outcome.failed += download.failed;
        outcome.failures.push(...download.failures);
        archivePaths = archivePaths.concat(download.completed_paths ?? []);
        updateCollectionTask({
          phase: "downloading",
          message: `已下载 ${batchIndex + 1}/${batches} 批，累计 ${archivePaths.length} 个曲包…`,
          processed: Math.min(items.length, outcome.completed + outcome.skipped + outcome.failed),
          total: items.length,
        });
        const paths = download.completed_paths ?? [];
        if (!paths.length) continue;
        hooks.onNotice?.(`正在解压本批 ${paths.length} 个曲包进 Songs（累计 ${archivePaths.length} 个）并计算收藏难度 MD5…`);
        updateCollectionTask({
          phase: "installing",
          message: `正在解压第 ${batchIndex + 1}/${batches} 批的 ${paths.length} 个曲包进 Songs…`,
          processed: 0,
          total: paths.length,
          errors: outcome.failures.map((failure) => `#${failure.beatmapset_id} ${failure.title}：${failure.message}`),
        });
        const install = await desktopApi.installCollectionDownloads(folderIds, paths);
        throwIfCollectionTaskCancelled();
        outcome.extractedSets += install.extracted_sets;
      }
    } finally {
      hooks.onDownloadActive?.(false);
    }
    if (!archivePaths.length && outcome.failed) {
      throw new Error("缺失曲包下载失败，请检查下载源后重试。");
    }
  }
  // 写回 collection.db：osu! 运行中会覆盖我们的写入，此时延期由全局监视器补写。
  hooks.onNotice?.("正在安全写回 osu!stable/collection.db…");
  updateCollectionTask({ phase: "writing", message: "曲包 MD5 已准备完成，正在统一写回 collection.db…", processed: 0, total: 1 });
  try {
    outcome.writtenFolders = await writeStableCollectionsWithRecovery();
  } catch (caught) {
    if ((caught as CommandError)?.code !== "GAME_RUNNING") throw caught;
    outcome.writeDeferred = true;
  }
  throwIfCollectionTaskCancelled();
  return outcome;
}

/** 把结果汇总成完成文案（各入口共用同一口径）。 */
export function describeStableImport(outcome: StableImportOutcome): string {
  if (outcome.writeDeferred) {
    return outcome.extractedSets || outcome.missingItems
      ? `${outcome.extractedSets} 个曲包已解压进 Songs（osu! 内 F5 刷新后即可游玩）；检测到 osu! 正在运行，收藏夹将在游戏退出后自动写回。`
      : "检测到 osu! 正在运行，收藏夹更改将在游戏退出后自动写回。";
  }
  const written = `已写回 ${outcome.writtenFolders ?? 0} 个收藏夹`;
  if (!outcome.missingItems) return `${written}，没有需要导入的缺失曲包。`;
  return `${written}，${outcome.extractedSets} 个曲包已解压进 Songs（osu! 内 F5 刷新后即可游玩）${outcome.failed ? `；${outcome.failed} 项下载失败` : ""}。`;
}

/** 已有收藏夹的轻量收尾（BeatmapHub 导入等场景）：确保参与 Stable 同步并把
 * 待写回变更落进 collection.db；游戏运行时同样延期自动补写。 */
export async function finalizeFolderToStable(folderId: string): Promise<StableImportOutcome> {
  const summaries = await desktopApi.listCollectionSummaries();
  const folder = summaries.folders.find((entry) => entry.id === folderId);
  if (folder?.stable_sync === false) await desktopApi.enableCollectionStableSync(folderId);
  await desktopApi.beginCollectionTask();
  beginCollectionTask({ phase: "writing", message: "正在把新收藏夹写回 osu!stable…", processed: 0, total: 1, errors: [] });
  const outcome: StableImportOutcome = {
    missingItems: 0, completed: 0, skipped: 0, failed: 0, failures: [],
    extractedSets: 0, writeDeferred: false, writtenFolders: null,
  };
  try {
    outcome.writtenFolders = await writeStableCollectionsWithRecovery();
  } catch (caught) {
    if ((caught as CommandError)?.code !== "GAME_RUNNING") throw caught;
    outcome.writeDeferred = true;
  }
  updateCollectionTask({ phase: "completed", message: describeStableImport(outcome), processed: 1, total: 1 });
  return outcome;
}
