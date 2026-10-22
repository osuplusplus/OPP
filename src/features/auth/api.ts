import { useQuery } from "@tanstack/react-query";
import { desktopApi, isTauri } from "../../shared/lib/tauri";

export const authQueryKey = ["auth-status"] as const;

export function useAuthStatus() {
  return useQuery({
    queryKey: authQueryKey,
    queryFn: () => isTauri() ? desktopApi.getAuthStatus() : Promise.resolve({
      credentials_configured: true,
      connected: true,
      client_id: "preview",
      callback_url: "http://127.0.0.1:1420/preview",
      user_id: 10001,
      username: "Preview User",
    }),
    staleTime: 15_000,
  });
}
