import { lazy, Suspense, useEffect, useState } from "react";
import type { CollectionCandidate } from "../../shared/types/osu";
import { NotificationCard } from "../../shared/components/notifications";
import { collectionAddEvent } from "./events";
const Content = lazy(() => import("./CollectionAddDialogContent"));

export function CollectionAddDialog({ defaultCreator = "" }: { defaultCreator?: string }) {
  const [request, setRequest] = useState<{ candidates: CollectionCandidate[]; version: number } | null>(null);
  useEffect(() => {
    let version = 0;
    const handler = (event: Event) => setRequest({ candidates: (event as CustomEvent<CollectionCandidate[]>).detail ?? [], version: ++version });
    window.addEventListener(collectionAddEvent, handler);
    return () => window.removeEventListener(collectionAddEvent, handler);
  }, []);
  return request ? <Suspense fallback={<NotificationCard className="fixed bottom-6 right-6 z-[250]" title="正在加载收藏夹…" tone="info" />}><Content key={request.version} candidates={request.candidates} defaultCreator={defaultCreator} onClose={() => setRequest(null)} /></Suspense> : null;
}
