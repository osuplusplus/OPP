import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { desktopApi } from "../../shared/lib/tauri";
import type { CommunityQuery } from "../../shared/types/osu";

export const communityKey = ["community"] as const;
export const communityApi = {
  save: desktopApi.saveCommunityLobby,
  close: desktopApi.closeCommunityLobby,
  delete: desktopApi.deleteCommunityLobby,
  openExternal: desktopApi.openExternal,
};
export function useTournaments(query: CommunityQuery) {
  return useInfiniteQuery({ queryKey: [...communityKey, "tournaments", query], initialPageParam: undefined as string | undefined,
    queryFn: ({ pageParam }) => desktopApi.listCommunityTournaments({ ...query, cursor: pageParam }),
    getNextPageParam: (page) => page.next_cursor ?? undefined, staleTime: 0, retry: false, refetchInterval: 60_000 });
}
export function useLobbies(query: CommunityQuery, mine: boolean, identity: string, enabled: boolean) {
  return useInfiniteQuery({ queryKey: [...communityKey, "lobbies", mine, identity, query], initialPageParam: undefined as string | undefined,
    queryFn: ({ pageParam }) => desktopApi.listCommunityLobbies({ ...query, cursor: pageParam }, mine),
    getNextPageParam: (page) => page.next_cursor ?? undefined, enabled, staleTime: 0, retry: false, refetchInterval: 60_000, gcTime: mine ? 0 : 5 * 60_000 });
}
export function useTournament(id: string | null) {
  return useQuery({ queryKey: [...communityKey, "tournament", id], queryFn: () => desktopApi.getCommunityTournament(id!), enabled: Boolean(id), staleTime: 0, retry: false });
}
export function useLobby(id: string | null) {
  return useQuery({ queryKey: [...communityKey, "lobby", id], queryFn: () => desktopApi.getCommunityLobby(id!), enabled: Boolean(id), staleTime: 0, retry: false });
}
export function useServerClock(serverTime: string | undefined, receivedAt: number) {
  const [clock, setClock] = useState(Date.now);
  useEffect(() => {
    if (!serverTime) return;
    const timer = window.setInterval(() => { if (document.visibilityState === "visible") setClock(Date.now()); }, 1000);
    return () => window.clearInterval(timer);
  }, [serverTime]);
  return serverTime ? new Date(new Date(serverTime).getTime() + Math.max(0, clock - receivedAt)) : new Date(clock);
}
