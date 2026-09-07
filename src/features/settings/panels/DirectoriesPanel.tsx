import { useState } from "react";
import { FolderOpen, RotateCcw } from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import { Card, SectionTitle, Button, Badge } from "../../../shared/components/ui";
import { localSourcesKey, useLocalSources } from "../../local-analysis/api";
import type { OsuClient } from "../../../shared/types/osu";
import { desktopApi } from "../../../shared/lib/tauri";

const clients: Array<[OsuClient, string]> = [
  ["stable", "osu! Stable"],
  ["lazer", "osu!lazer"],
];

export function DirectoriesPanel() {
  const queryClient = useQueryClient();
  const sources = useLocalSources();
  const [sourceBusy, setSourceBusy] = useState<OsuClient | null>(null);

  const chooseSource = async (client: OsuClient) => {
    setSourceBusy(client);
    try {
      const current = sources.data?.find((item) => item.client === client);
      const currentPath =
        current?.configured_path ?? current?.data_root ?? current?.install_root;
      const selected = await desktopApi.chooseLocalDirectory(currentPath);
      if (selected) {
        await desktopApi.setLocalSource(client, selected);
        await queryClient.invalidateQueries({ queryKey: localSourcesKey });
      }
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
    <div className="space-y-5">
      <div>
        <h2 className="text-2xl font-bold text-white">游戏目录</h2>
        <p className="mt-1 text-sm text-slate-400">
          为 Stable 和 lazer 分别选择 osu! 安装或数据目录，保存后会重新建立本地资源索引。
        </p>
      </div>

      <Card className="p-6">
        <SectionTitle
          title="osu! 安装目录"
          description="设置游戏路径以访问本地谱面、回放、皮肤等资源。"
        />

        <div className="mt-5 space-y-3">
          {clients.map(([client, label]) => {
            const source = sources.data?.find((item) => item.client === client);
            const path =
              source?.configured_path ??
              source?.data_root ??
              source?.install_root;

            return (
              <div
                className="rounded-xl border border-white/[0.1] bg-white/[0.035] p-4"
                key={client}
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <p className="font-semibold text-slate-100">{label}</p>
                    <p
                      className="mt-1 truncate text-xs text-slate-400"
                      title={path ?? undefined}
                    >
                      {path ?? "未选择目录，将自动检测"}
                    </p>
                  </div>
                  <Badge tone={source?.valid ? "success" : "warning"}>
                    {source?.valid ? "可用" : "未配置"}
                  </Badge>
                </div>

                <div className="mt-3 flex flex-wrap gap-2">
                  <Button
                    loading={sourceBusy === client}
                    onClick={() => void chooseSource(client)}
                    size="sm"
                    variant="secondary"
                  >
                    <FolderOpen className="size-4" />
                    选择目录
                  </Button>

                  <Button
                    disabled={sourceBusy !== null}
                    onClick={() => void resetSource(client)}
                    size="sm"
                    variant="ghost"
                  >
                    <RotateCcw className="size-4" />
                    自动检测
                  </Button>
                </div>
              </div>
            );
          })}
        </div>
      </Card>

      <div className="rounded-lg border border-white/[0.06] bg-white/[0.02] p-4">
        <p className="text-xs text-slate-400">
          修改路径后，OPP 会自动重新扫描和索引游戏资源。
          <br />
          如果自动检测失败，请手动选择 osu!
          的安装目录（包含可执行文件的文件夹）。
        </p>
      </div>
    </div>
  );
}
