import { useQuery } from "@tanstack/react-query";
import { Music2 } from "lucide-react";
import { useState } from "react";
import { desktopApi } from "../../shared/lib/tauri";
import type { CollectionArtwork } from "../../shared/types/osu";

export function CollectionThumbnail({ item, beatmapsetId }: { item: CollectionArtwork | null; beatmapsetId?: number | null }) {
  const [failed, setFailed] = useState<string | null>(null);
  const image = useQuery({ queryKey: ["local-beatmap-background", item?.client, item?.resource_id, "thumbnail"], queryFn: () => desktopApi.getLocalBeatmapBackground(item!.client, item!.resource_id, "thumbnail"), enabled: !!item, staleTime: Infinity, gcTime: 60_000, retry: false });
  const source = image.data || (beatmapsetId && beatmapsetId > 0 ? `https://assets.ppy.sh/beatmaps/${beatmapsetId}/covers/list.jpg` : null);
  return <span className="collection-thumbnail">{source && source !== failed ? <img alt="" loading="lazy" src={source} onError={() => setFailed(source)} /> : <Music2 size={20} />}</span>;
}
