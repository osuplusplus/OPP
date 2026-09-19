import { lazy, Suspense, useEffect, useState } from "react";
import type { CollectionCandidate } from "../../shared/types/osu";
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
  return request ? <Suspense fallback={<p role="status" className="fixed bottom-6 right-6 z-[250] rounded-xl bg-slate-900 p-4 text-sm text-white">正在加载收藏夹…</p>}><Content key={request.version} candidates={request.candidates} defaultCreator={defaultCreator} onClose={() => setRequest(null)} /></Suspense> : null;
}
