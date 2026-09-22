import { useQuery } from "@tanstack/react-query";
import { Music2 } from "lucide-react";
import { desktopApi } from "../../shared/lib/tauri";
import type { CollectionArtwork } from "../../shared/types/osu";

export function CollectionThumbnail({ item }: { item: CollectionArtwork | null }) {
  const image = useQuery({ queryKey: ["local-beatmap-background", item?.client, item?.resource_id, "thumbnail"], queryFn: () => desktopApi.getLocalBeatmapBackground(item!.client, item!.resource_id, "thumbnail"), enabled: !!item, staleTime: Infinity, gcTime: 60_000, retry: false });
  return <span className="collection-thumbnail">{image.data ? <img alt="" src={image.data} onError={(event) => { event.currentTarget.hidden = true; }} /> : <Music2 size={20} />}</span>;
}
