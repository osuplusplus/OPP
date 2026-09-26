import type { GameMediaItem, ReplayMapInfo } from "../../shared/types/osu";

export type RenderProvider = "live" | "danser" | "ordr";
export function replayName(path: string) { return path.split(/[\\/]/).pop() || path; }
export function mergeReplays(local: GameMediaItem[], selected: GameMediaItem[]) {
  return [...new Map([...local.filter((item) => item.kind === "replay"), ...selected].map((item) => [item.path, item])).values()];
}
export function matchesReplay(item: GameMediaItem, query: string, info?: ReplayMapInfo) {
  return `${item.path} ${info?.beatmap_title ?? ""} ${info?.username ?? ""}`.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase());
}
export function replayBlockReason(path: string, info: ReplayMapInfo | null | undefined, provider: RenderProvider, error?: unknown) {
  if (!path) return "请从素材库或文件选择器选择回放";
  if (error) return typeof error === "object" && "message" in error ? String(error.message) : "回放读取失败，请重新选择文件或重试";
  if (!info) return "正在匹配回放…";
  if (info.ruleset && info.ruleset !== "osu") return "当前渲染方式仅支持 osu!standard 回放";
  if (!info.beatmap_resource_id) return "未匹配本地谱面，请安装并扫描后重新匹配";
  if (provider === "ordr" && !info.submitted) return "o!rdr 需要已提交到 osu! 的谱面";
  return null;
}
