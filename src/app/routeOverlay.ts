import { createContext } from "react";

export const RouteOverlayContext = createContext<(() => void) | null>(null);

export function isOverlayRoute(pathname: string) {
  return pathname === "/settings" || pathname === "/tools" || /^\/tools\/(game|beatmaps|system|live)$/.test(pathname);
}
