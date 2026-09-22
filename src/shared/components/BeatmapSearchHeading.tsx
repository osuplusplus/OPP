import type { ReactNode } from "react";
import "../styles/beatmapSearch.css";

export function BeatmapSearchHeading({ icon, title, description }: { icon: ReactNode; title: string; description: string }) {
  return <div className="beatmap-search-heading">
    <span aria-hidden="true" className="beatmap-search-heading-icon">{icon}</span>
    <h1>{title}</h1>
    <p>{description}</p>
  </div>;
}
