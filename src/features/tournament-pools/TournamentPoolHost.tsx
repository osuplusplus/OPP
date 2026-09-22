import { useEffect, useRef, useSyncExternalStore } from "react";
import { useNavigate } from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";
import { desktopApi } from "../../shared/lib/tauri";
import type { TournamentLink } from "../../shared/types/osu";
import { collectionEntriesKey, collectionsQueryKey } from "../collections/api";
import { poolImportSession } from "./importSession";

/** Mounted after authentication; Rust retains links while startup/login is in progress. */
export function TournamentPoolHost({ session = poolImportSession }: { session?: typeof poolImportSession }) {
  const navigate = useNavigate();
  const client = useQueryClient();
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot);
  const completed = useRef(0);
  useEffect(() => {
    let disposed = false;
    const disposers: (() => void)[] = [];
    const report = (error: unknown) => { void desktopApi.writeClientLog("error", "tournament.uri", String(error)); };
    const keep = (dispose: () => void) => { if (disposed) dispose(); else disposers.push(dispose); };
    const receive = (incoming: TournamentLink | null) => {
      if (disposed || !incoming || !session.receive(incoming)) return;
      navigate("/collections");
      void desktopApi.acknowledgeTournamentLink(incoming.id).catch(report);
    };
    void (async () => {
      keep(await desktopApi.onTournamentImportProgress(session.progress));
      if (disposed) return;
      keep(await desktopApi.onTournamentPoolOpen(receive));
      if (!disposed) receive(await desktopApi.getPendingTournamentLink());
    })().catch(report);
    return () => { disposed = true; disposers.forEach((dispose) => dispose()); };
  }, [navigate, session]);
  useEffect(() => {
    if (state?.phase !== "completed" || !state.folderId || state.link.id <= completed.current) return;
    completed.current = state.link.id;
    const folderId = state.folderId;
    void Promise.all([
      client.invalidateQueries({ queryKey: collectionsQueryKey }),
      client.invalidateQueries({ queryKey: collectionEntriesKey(folderId) }),
    ]).then(() => {
      if (session.getSnapshot()?.link.id === state.link.id) navigate(`/collections?${new URLSearchParams({ folder: folderId })}`);
    });
  }, [client, navigate, session, state]);
  return null;
}
