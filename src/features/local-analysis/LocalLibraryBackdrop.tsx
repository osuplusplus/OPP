import { ArtworkMosaic } from "../../shared/components/ArtworkMosaic";
import { useSettings } from "../settings/api";
import { useLocalArtworkSample } from "./api";

export function LocalLibraryBackdrop() {
  const candidates = useLocalArtworkSample();
  const settings = useSettings();
  return <ArtworkMosaic candidates={candidates.data ?? []} animated={!settings.data?.reduce_motion} />;
}
