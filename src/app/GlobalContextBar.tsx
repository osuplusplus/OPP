import { useCallback, useEffect, useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Database, Settings2 } from "lucide-react";
import { useLocation, useNavigate } from "react-router-dom";
import { ClientSwitch } from "../shared/components/ClientSwitch";
import { ModeSwitch } from "../shared/components/ModeSwitch";
import { Button } from "../shared/components/ui";
import { desktopApi } from "../shared/lib/tauri";
import type { CommandError, GameStatusSnapshot, LocalScanProgress, OsuClient } from "../shared/types/osu";
import { useMode } from "./ModeContext";

const routeContexts = [
  ["/data", "数据中心"],
  ["/online/overview", "个人概览"],
  ["/online/scores", "最佳成绩"],
  ["/online/profile", "详细档案"],
  ["/online/beatmaps", "在线谱面"],
  ["/online/similar", "相似谱面"],
  ["/collections", "谱面收藏夹"],
  ["/local/maps", "本地谱面"],
  ["/local/skins", "本地皮肤"],
  ["/local/media", "截图与回放"],
  ["/tosu", "tosu 直播集成"],
  ["/tools", "工具集合"],
  ["/settings", "设置"],
] as const;

const localSourcesKey = ["local-sources"] as const;
const localSummaryKey = (client: OsuClient) => ["local-summary", client] as const;

function GlobalLocalScanAction({ client }: { client: OsuClient }) {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [scanning, setScanning] = useState(false);
  const [progress, setProgress] = useState<LocalScanProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const sourcesQuery = useQuery({
    queryKey: localSourcesKey,
    queryFn: desktopApi.getLocalSources,
    staleTime: 30_000,
    retry: false,
  });
  const summaryQuery = useQuery({
    queryKey: localSummaryKey(client),
    queryFn: () => desktopApi.getLocalSummary(client),
    retry: false,
  });
  const source = sourcesQuery.data?.find((item) => item.client === client);

  const refreshLocalData = useCallback(async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: localSourcesKey }),
      queryClient.invalidateQueries({ queryKey: localSummaryKey(client) }),
      queryClient.invalidateQueries({ queryKey: ["local-beatmaps"] }),
      queryClient.invalidateQueries({ queryKey: ["local-beatmap-sets"] }),
      queryClient.invalidateQueries({ queryKey: ["local-skins"] }),
      queryClient.invalidateQueries({ queryKey: ["local-skin-detail"] }),
      queryClient.invalidateQueries({ queryKey: ["local-skin-preview"] }),
      queryClient.invalidateQueries({ queryKey: ["skin-workshop-tree"] }),
    ]);
  }, [client, queryClient]);

  useEffect(() => {
    let dispose: () => void = () => undefined;
    void desktopApi.onLocalScanProgress((event) => {
      if (event.client !== client) return;
      setScanning(event.percent < 100);
      setProgress(event);
      if (event.percent >= 100) void refreshLocalData();
    }).then((unlisten) => {
      dispose = unlisten;
    });
    return () => dispose();
  }, [client, refreshLocalData]);

  const scan = async () => {
    if (!source?.valid) {
      navigate("/local/maps");
      return;
    }
    setScanning(true);
    setProgress(null);
    setError(null);
    try {
      const summary = await desktopApi.scanLocalSource(client, false);
      queryClient.setQueryData(localSummaryKey(client), summary);
      await refreshLocalData();
    } catch (caught) {
      const commandError = caught as CommandError;
      if (commandError.code !== "SCAN_CANCELLED") {
        setError(commandError.message ?? String(caught));
      }
    } finally {
      setScanning(false);
    }
  };

  if (summaryQuery.isLoading || sourcesQuery.isLoading || summaryQuery.data) return null;

  const needsConfiguration = !source?.valid;
  return (
    <Button
      disabled={scanning}
      loading={scanning}
      onClick={() => void scan()}
      size="sm"
      title={error ?? (needsConfiguration ? source?.validation_errors[0] ?? "请先配置本地数据源" : "扫描当前客户端的本地谱面与 Skin")}
      variant={error || needsConfiguration ? "secondary" : "primary"}
    >
      {scanning ? null : needsConfiguration ? <Settings2 className="size-3.5" /> : <Database className="size-3.5" />}
      {scanning
        ? `扫描中 ${Math.round(progress?.percent ?? 0)}%`
        : error
          ? "扫描失败，重试"
          : needsConfiguration
            ? "配置数据源"
            : "扫描本地数据"}
    </Button>
  );
}

export function GlobalContextBar() {
  const { client, setClient, ruleset, setRuleset } = useMode();
  const location = useLocation();
  const [gameStatus, setGameStatus] = useState<GameStatusSnapshot | null>(null);
  const current =
    routeContexts.find(([path]) => location.pathname === path) ??
    routeContexts[0];
  const runningClients = useMemo(
    () => gameStatus?.clients.filter((item) => item.running) ?? [],
    [gameStatus],
  );

  useEffect(() => {
    let disposed = false;
    let off: (() => void) | undefined;
    const update = (status: GameStatusSnapshot) => {
      if (!disposed) setGameStatus(status);
    };
    void desktopApi.getGameStatus().then(update).catch(() => undefined);
    void desktopApi.onGameStatusChanged(update).then((unlisten) => {
      if (disposed) unlisten();
      else off = unlisten;
    });
    return () => {
      disposed = true;
      off?.();
    };
  }, []);

  return (
    <header className="theme-context-bar fixed left-[var(--sidebar-width)] right-0 top-11 z-30 h-16 border-b border-[var(--line-subtle)] px-7 xl:px-9">
      <div className="mx-auto flex h-full max-w-[var(--content-width)] items-center gap-5">
        <p className="min-w-0 truncate text-[15px] font-semibold text-slate-200">
          {current[1]}
        </p>
        <div
          className="flex min-h-8 items-center gap-2 border-l border-[var(--line-subtle)] px-3 text-xs font-medium text-slate-400"
          title={runningClients.map((item) => item.executable).filter(Boolean).join("\n") || "未检测到运行中的 osu! 客户端"}
        >
          <span
            aria-hidden="true"
            className={`size-2 rounded-full ${runningClients.length ? "bg-emerald-400 shadow-[0_0_0_4px_rgba(52,211,153,0.12)]" : "bg-slate-500"}`}
          />
          {runningClients.length
            ? `${runningClients.map((item) => item.client === "stable" ? "Stable" : "Lazer").join(" + ")} 运行中`
            : "游戏未运行"}
        </div>
        <span className="ml-auto h-6 w-px bg-white/[0.08]" />
        <GlobalLocalScanAction client={client} />
        <div className="flex items-center gap-5" data-onboarding="mode-and-client">
          <div aria-label="游戏模式" className="flex items-center">
            <ModeSwitch compact onChange={setRuleset} value={ruleset} />
          </div>
          <div aria-label="osu! 客户端" className="flex items-center">
            <ClientSwitch onChange={setClient} value={client} />
          </div>
        </div>
      </div>
    </header>
  );
}
