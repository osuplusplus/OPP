import { ArtworkMosaic } from "../../shared/components/ArtworkMosaic";
import { useCollectionArtwork } from "./api";

const startupOffset = Math.floor(Math.random() * 1_000_000);

export function CollectionBackdrop({ folderId, animated }: { folderId: string | null; animated: boolean }) {
  const candidates = useCollectionArtwork(folderId, startupOffset);
  return <ArtworkMosaic candidates={candidates.data ?? []} animated={animated} />;
}
