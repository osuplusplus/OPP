import { createContext, useCallback, useContext, useEffect, useRef } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useSearchParams } from "react-router-dom";
import { useMode } from "../../app/ModeContext";
import { desktopApi } from "../../shared/lib/tauri";
import type { GameMediaItem, OsuClient } from "../../shared/types/osu";
import { mergeReplays } from "./model";

export function useRenderSession<T>(key: string, initial: T) {
  const queryClient = useQueryClient();
  const queryKey = ["replay-studio-session", key];
  const { data } = useQuery({ queryKey, queryFn: () => initial, initialData: initial, enabled: false, gcTime: Infinity, staleTime: Infinity });
  const set = useCallback((value: T | ((previous: T) => T)) => queryClient.setQueryData<T>(["replay-studio-session", key], (previous) => typeof value === "function" ? (value as (previous: T) => T)(previous === undefined ? initial : previous) : value), [queryClient, key, initial]);
  return [data === undefined ? initial : data, set] as const;
}

interface LibrarySession { imported: GameMediaItem[]; current: string; checked: string[] }
const emptySession: LibrarySession = { imported: [], current: "", checked: [] };
export const replayInfoKey = (client: OsuClient, path: string) => ["replay-studio-info", client, path];

let inspectingRows = 0;
const inspectionQueue: Array<() => void> = [];
/** Keep browsing a large library from issuing dozens of simultaneous file reads. */
export async function inspectLibraryReplay(client: OsuClient, path: string, signal: AbortSignal) {
  await new Promise<void>((resolve) => {
    const begin = () => { inspectingRows++; resolve(); };
    if (inspectingRows < 4) begin(); else inspectionQueue.push(begin);
  });
  try {
    signal.throwIfAborted();
    return await desktopApi.inspectGameReplay(client, path);
  } finally {
    inspectingRows--;
    inspectionQueue.shift()?.();
  }
}

export function useReplayLibrary() {
  const { client } = useMode();
  const queryClient = useQueryClient();
  const [params] = useSearchParams();
  const requested = params.get("replay");
  const appliedLink = useRef("");
  const [session, setSession] = useRenderSession(`library:${client}`, emptySession);
  const media = useQuery({ queryKey: ["replay-studio-media", client], queryFn: () => desktopApi.listGameMedia(client) });
  const items = mergeReplays(media.data ?? [], session.imported);
  const replayPath = session.current || (items.some((item) => item.path === requested) ? requested! : items[0]?.path ?? "");
  useEffect(() => {
    const linkKey = `${client}:${requested ?? ""}`;
    if (requested && appliedLink.current !== linkKey && items.some((item) => item.path === requested)) {
      appliedLink.current = linkKey;
      queryClient.setQueryData<LibrarySession>(["replay-studio-session", `library:${client}`], (previous) => ({ ...(previous ?? emptySession), current: requested }));
    }
    // Apply deep links once the local list becomes available, not on each selection.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [client, requested, media.data]);
  const inspection = useQuery({ queryKey: replayInfoKey(client, replayPath), queryFn: () => desktopApi.inspectGameReplay(client, replayPath), enabled: Boolean(replayPath), retry: false, staleTime: 30_000 });
  const select = (path: string) => setSession((current) => ({ ...current, current: path }));
  const chooseFiles = async () => {
    const result = await desktopApi.chooseGameReplayFiles(client);
    if (result.items.length) setSession((current) => ({ ...current, imported: mergeReplays(current.imported, result.items), current: result.items[0].path }));
    await queryClient.invalidateQueries({ queryKey: ["replay-studio-info", client] });
    return result;
  };
  const toggleChecked = (path: string) => setSession((current) => ({ ...current, checked: current.checked.includes(path) ? current.checked.filter((entry) => entry !== path) : [...current.checked, path] }));
  const refresh = async () => {
    await Promise.all([media.refetch(), queryClient.invalidateQueries({ queryKey: ["replay-studio-info", client] })]);
  };
  return { client, items, replayPath, replayInfo: inspection.error ? null : inspection.data ?? null, inspecting: inspection.isFetching, inspectError: inspection.error, loading: media.isPending, listError: media.error, imported: session.imported, checked: session.checked, select, chooseFiles, toggleChecked, refresh };
}

export type ReplayWorkspace = ReturnType<typeof useReplayLibrary>;
export const ReplayWorkspaceContext = createContext<ReplayWorkspace | null>(null);
export function useReplayWorkspace() {
  const value = useContext(ReplayWorkspaceContext);
  if (!value) throw new Error("Replay workspace is missing");
  return value;
}
