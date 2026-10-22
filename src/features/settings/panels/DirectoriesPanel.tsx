import { useState } from "react";
import { FolderOpen, RotateCcw } from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import { Badge, Button, Card, SectionTitle } from "../../../shared/components/ui";
import { desktopApi } from "../../../shared/lib/tauri";
import type { OsuClient } from "../../../shared/types/osu";
import { localSourcesKey, useLocalSources } from "../../local-analysis/api";

const clients: Array<[OsuClient, string]> = [
  ["stable", "osu! Stable"],
  ["lazer", "osu!lazer"],
];

export function DirectoriesPanel({ className }: { className?: string }) {
  const sources = useLocalSources();
  const queryClient = useQueryClient();
  const [sourceBusy, setSourceBusy] = useState<OsuClient | null>(null);

  const chooseSource = async (client: OsuClient) => {
    setSourceBusy(client);
    try {
      const source = sources.data?.find((item) => item.client === client);
      const selected = await desktopApi.chooseLocalDirectory(
        source?.configured_path ?? source?.install_root ?? source?.data_root,
      );
      if (!selected) return;
      await desktopApi.setLocalSource(client, selected);
      await queryClient.invalidateQueries({ queryKey: localSourcesKey });
    } finally {
      setSourceBusy(null);
    }
  };

  const resetSource = async (client: OsuClient) => {
    setSourceBusy(client);
    try {
      await desktopApi.resetLocalSource(client);
      await queryClient.invalidateQueries({ queryKey: localSourcesKey });
    } finally {
      setSourceBusy(null);
    }
  };

  return (
    <Card className={className}>
      <SectionTitle title="游戏目录" description="为 Stable 和 lazer 分别选择 osu! 安装或数据目录，保存后会重新建立本地资源索引。" />
      <div className="mt-5 space-y-3">
        {clients.map(([client, label]) => {
          const source = sources.data?.find((item) => item.client === client);
          const path = source?.configured_path ?? source?.data_root ?? source?.install_root;
          return (
            <div className="rounded-xl border border-white/[.1] bg-white/[.035] p-4" key={client}>
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <p className="font-semibold text-slate-100">{label}</p>
                  <p className="mt-1 truncate text-xs text-slate-400" title={path ?? undefined}>
                    {path ?? "未选择目录，将自动检测"}
                  </p>
                </div>
                <Badge tone={source?.valid ? "success" : "warning"}>
                  {source?.valid ? "可用" : "未配置"}
                </Badge>
              </div>
              <div className="mt-3 flex flex-wrap gap-2">
                <Button loading={sourceBusy === client} onClick={() => void chooseSource(client)} size="sm" variant="secondary">
                  <FolderOpen className="size-4" />选择目录
                </Button>
                <Button disabled={sourceBusy !== null} onClick={() => void resetSource(client)} size="sm" variant="ghost">
                  <RotateCcw className="size-4" />自动检测
                </Button>
              </div>
            </div>
          );
        })}
      </div>
    </Card>
  );
}
