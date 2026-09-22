import { useEffect } from "react";
import { markInteractive } from "../shared/lib/performance";
import { ModeProvider } from "./ModeContext";
import { AppRoutes } from "./AppRoutes";
import { AppLoading } from "./AppLoading";
import { AuthSetup } from "../features/auth/AuthSetup";
import { useAuthStatus } from "../features/auth/api";
import { ErrorPanel } from "../shared/components/ErrorPanel";
import { TournamentPoolHost } from "../features/tournament-pools/TournamentPoolHost";
import { useLocalArtworkSample } from "../features/local-analysis/api";

function ConnectedApplication() {
  useLocalArtworkSample();
  return (
    <ModeProvider>
      <AppRoutes />
      <TournamentPoolHost />
    </ModeProvider>
  );
}

/** Resolves desktop authentication before mounting feature routes. */
export function AppConnectionGate() {
  const auth = useAuthStatus();
  useEffect(() => { if (!auth.isLoading) markInteractive(); }, [auth.isLoading]);

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
