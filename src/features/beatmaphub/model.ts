import type { CommandError } from "../../shared/types/osu";

export function hubError(error: unknown) {
  const value = error as CommandError | null;
  return `${value?.message ?? String(error)}${value?.request_id ? `（请求 ID：${value.request_id}）` : ""}`;
}
export function parseHubInput(raw: string): { kind: "code"; id: string; bare: boolean } | { kind: "search"; query: string } | { kind: "invalid" } {
  const text = raw.trim();
  const prefixed = /^BPH-/i.test(text);
  const id = text.replace(/^BPH-/i, "").toUpperCase();
  if (/^[23456789ABCDEFGHJKMNPQRSTUVWXYZ]{6}$/.test(id)) return { kind: "code", id, bare: !prefixed };
  return prefixed ? { kind: "invalid" } : { kind: "search", query: text };
}
export function packCover(ids: number[]) {
  const id = ids.find((value) => Number.isSafeInteger(value) && value > 0);
  return id ? `https://assets.ppy.sh/beatmaps/${id}/covers/cover.jpg` : null;
}
export async function copyHubCode(code: string) {
  try {
    if (!navigator.clipboard) return false;
    await navigator.clipboard.writeText(code);
    return true;
  } catch { return false; }
}
export interface CommentDraft { content: string; editing: string | null }
