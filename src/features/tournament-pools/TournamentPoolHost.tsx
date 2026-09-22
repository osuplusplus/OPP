import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { desktopApi } from "../../shared/lib/tauri";
import type { TournamentLink } from "../../shared/types/osu";
import { NotificationCard } from "../../shared/components/notifications";

const TournamentPoolDialog = lazy(() => import("./TournamentPoolDialog"));

/** Mounted after authentication; the Rust inbox retains links while startup/login is in progress. */
export function TournamentPoolHost() {
  const [link, setLink] = useState<TournamentLink | null>(null);
  const lastId = useRef(0);
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    const report = (error: unknown) => { void desktopApi.writeClientLog("error", "tournament.uri", String(error)); };
    const receive = (incoming: TournamentLink | null) => {
      if (disposed || !incoming || incoming.id <= lastId.current) return;
      lastId.current = incoming.id;
      setLink(incoming);
      void desktopApi.acknowledgeTournamentLink(incoming.id).catch(report);
    };
    // Subscribe before draining the inbox; generation IDs prevent an older response overriding a live event.
    void desktopApi.onTournamentPoolOpen(receive).then((dispose) => {
      if (disposed) { dispose(); return; }
      unlisten = dispose;
      return desktopApi.getPendingTournamentLink().then(receive);
    }).catch(report);
    return () => { disposed = true; unlisten?.(); };
  }, []);
  return link ? <Suspense fallback={<NotificationCard className="fixed bottom-6 right-6 z-[250]" title="正在打开比赛图池…" tone="info" />}>
    <TournamentPoolDialog key={link.id} reference={link.reference} onClose={() => setLink(null)} />
  </Suspense> : null;
}
