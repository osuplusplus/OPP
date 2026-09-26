import { useCallback, useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Database, Settings2 } from "lucide-react";
import { Button } from "../../shared/components/ui";
import { desktopApi } from "../../shared/lib/tauri";
import type { CommandError, LocalScanProgress, OsuClient } from "../../shared/types/osu";
import { useLocalSummary } from "../local-analysis/api";

const localSourcesKey = ["local-sources"] as const;
const localSummaryKey = (client: OsuClient) => ["local-summary", client] as const;

export function LocalScanAction({ client, onConfigure, allowRescan = false }: { client: OsuClient; onConfigure: () => void; allowRescan?: boolean }) {
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
  const summaryQuery = useLocalSummary(client);
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
      queryClient.invalidateQueries({ queryKey: ["local-index-status"] }),
      queryClient.invalidateQueries({ queryKey: ["local-library-storage"] }),
      queryClient.invalidateQueries({ queryKey: ["local-beatmap-presence"] }),
    ]);
  }, [client, queryClient]);

  useEffect(() => {
    let disposed = false;
    let dispose: () => void = () => undefined;
    void desktopApi.onLocalScanProgress((event) => {
      if (event.client !== client) return;
      setScanning(event.percent < 100);
      setProgress(event);
      if (event.percent >= 100) void refreshLocalData();
    }).then((unlisten) => {
      if (disposed) unlisten();
      else dispose = unlisten;
    }).catch((caught: unknown) => {
      if (!disposed) setError((caught as CommandError).message ?? "无法获取扫描进度");
    });
    return () => { disposed = true; dispose(); };
  }, [client, refreshLocalData]);

  const scan = async () => {
    if (!source?.valid) {
      onConfigure();
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

  if (summaryQuery.isPending || sourcesQuery.isLoading || (!allowRescan && summaryQuery.data)) return null;

  const needsConfiguration = !source?.valid;
  return (
    <Button
      className="shrink-0 whitespace-nowrap"
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
            : summaryQuery.data ? "更新本地索引" : "扫描本地数据"}
    </Button>
  );
}

