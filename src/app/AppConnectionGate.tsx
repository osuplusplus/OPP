import { useEffect, useRef } from "react";
import { markInteractive } from "../shared/lib/performance";
import { ModeProvider, useMode } from "./ModeContext";
import { AppRoutes } from "./AppRoutes";
import { AppLoading } from "./AppLoading";
import { AuthSetup } from "../features/auth/AuthSetup";
import { useAuthStatus } from "../features/auth/api";
import { ErrorPanel } from "../shared/components/ErrorPanel";
import { TournamentPoolHost } from "../features/tournament-pools/TournamentPoolHost";
import { useLocalArtworkSample } from "../features/local-analysis/api";
import { desktopApi, isTauri } from "../shared/lib/tauri";
import { beatmapHubAuthKey } from "../features/beatmaphub/api";
import { useQueryClient } from "@tanstack/react-query";

function ConnectedApplication() {
  useLocalArtworkSample();
  return (
    <ModeProvider>
      <CareerBootstrap />
      <AppRoutes />
      <TournamentPoolHost />
    </ModeProvider>
  );
}

function CareerBootstrap() {
  const { ruleset } = useMode();
  const auth = useAuthStatus();
  const queryClient = useQueryClient();
  const captured = useRef<string | null>(null);
  const captureKey = `${auth.data?.user_id ?? "unknown"}:${ruleset}`;
  useEffect(() => {
    if (!isTauri() || !auth.data?.user_id || captured.current === captureKey) return;
    captured.current = captureKey;
    void desktopApi.captureCareerSnapshot(ruleset).then(() => {
      void queryClient.invalidateQueries({ queryKey: ["career-calendar", ruleset] });
      void queryClient.invalidateQueries({ queryKey: ["career-status"] });
    }).catch(() => {
      void queryClient.invalidateQueries({ queryKey: ["career-calendar", ruleset] });
      void queryClient.invalidateQueries({ queryKey: ["career-status"] });
    });
  }, [auth.data?.user_id, captureKey, queryClient, ruleset]);
  return null;
}

/** Resolves desktop authentication before mounting feature routes. */
export function AppConnectionGate() {
  const auth = useAuthStatus();
  const queryClient = useQueryClient();
  const bootstrapped = useRef(false);
  useEffect(() => { if (!auth.isLoading) markInteractive(); }, [auth.isLoading]);
  useEffect(() => {
    if (!auth.data?.connected) {
      bootstrapped.current = false;
      return;
    }
    if (!isTauri() || bootstrapped.current) return;
    bootstrapped.current = true;
    void desktopApi.bootstrapBeatmapHub()
      .then(() => queryClient.invalidateQueries({ queryKey: beatmapHubAuthKey }))
      .catch(() => undefined);
  }, [auth.data?.connected, queryClient]);

  if (auth.isLoading) return <AppLoading />;
  if (auth.error || !auth.data) {
    return (
      <main className="mx-auto grid min-h-screen max-w-2xl place-items-center px-8">
        <div className="w-full">
          <ErrorPanel error={auth.error} onRetry={() => auth.refetch()} />
        </div>
      </main>
    );
  }
  if (!auth.data.connected) return <AuthSetup status={auth.data} />;

  return <ConnectedApplication />;
}
