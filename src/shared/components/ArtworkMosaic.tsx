import { useEffect, useState } from "react";
import { useQueries } from "@tanstack/react-query";
import { desktopApi } from "../lib/tauri";
import type { LocalArtwork } from "../types/osu";
import { mosaicTiles } from "../lib/artworkMosaic";
import "../styles/artworkMosaic.css";

/** Only thumbnail references are accepted; never decode the full local library. */
export function ArtworkMosaic({ candidates, animated = true }: { candidates: LocalArtwork[]; animated?: boolean }) {
  const [visible, setVisible] = useState(() => !document.hidden);
  const [reduced, setReduced] = useState(() => window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false);
  useEffect(() => {
    const media = window.matchMedia?.("(prefers-reduced-motion: reduce)");
    const visibility = () => setVisible(!document.hidden);
    const motion = () => setReduced(media?.matches ?? false);
    document.addEventListener("visibilitychange", visibility); media?.addEventListener("change", motion);
    return () => { document.removeEventListener("visibilitychange", visibility); media?.removeEventListener("change", motion); };
  }, []);
  const distinct = [...new Map(candidates.map(item => [`${item.client}:${item.resource_id}`, item])).values()].slice(0, 24);
  const images = useQueries({ queries: distinct.map((item) => ({ queryKey: ["local-mosaic-image", item.client, item.resource_id], queryFn: () => desktopApi.getLocalBeatmapBackground(item.client, item.resource_id, "thumbnail"), staleTime: Infinity, gcTime: 5 * 60_000, retry: false, enabled: visible })) });
  const urls = mosaicTiles(images.flatMap(image => image.data ? [image.data] : []));
  return <div className="artwork-mosaic-backdrop" aria-hidden="true"><div className={`artwork-mosaic-grid ${animated && visible && !reduced ? "is-moving" : ""}`}>
    {urls.map((url, i) => <img src={url} alt="" key={`${i}:${url.slice(-48)}`} onError={event => { event.currentTarget.hidden = true; }} />)}
  </div><div className="artwork-mosaic-shade" /></div>;
}
