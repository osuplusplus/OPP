import { createContext } from "react";
import type { RenderProvider } from "./model";

export const StudioPanelsContext = createContext<{ host: HTMLDivElement | null; provider: RenderProvider }>({ host: null, provider: "live" });
