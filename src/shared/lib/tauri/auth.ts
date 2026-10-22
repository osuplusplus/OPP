import type { AuthStatus, DisconnectResult, PendingOAuth } from "../../types/osu";
import { call } from "./invoke";

// auth 域的 Tauri 命令适配（从 lib/tauri.ts 拆分，desktopApi 组合导出不变）。
export const authApi = {
  getAuthStatus: () => call<AuthStatus>("get_auth_status"),

  saveOAuthCredentials: (clientId: string, clientSecret: string) =>
    call<{ client_id: string; callback_url: string }>(
      "save_oauth_credentials",
      { clientId, clientSecret },
    ),

  beginOAuthLogin: () => call<PendingOAuth>("begin_oauth_login"),

  cancelOAuthLogin: () => call<void>("cancel_oauth_login"),

  disconnectOsu: (revoke = true) =>
    call<DisconnectResult>("disconnect_osu", { revoke }),

  clearProfileCache: () => call<void>("clear_profile_cache"),
};
