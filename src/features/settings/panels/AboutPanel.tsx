import { ExternalLink, Users } from "lucide-react";
import { Card, SectionTitle, Button, DataLine } from "../../../shared/components/ui";
import { desktopApi } from "../../../shared/lib/tauri";
import {
  COMMUNITY_GROUP_MESSAGE,
  COMMUNITY_GROUP_NUMBER,
} from "../../../shared/constants/community";

// 从 package.json 获取版本号（编译时注入）
const APP_VERSION = "0.4.6";

export function AboutPanel() {
  return (
    <div className="space-y-5">
      <div>
        <h2 className="text-2xl font-bold text-white">关于</h2>
        <p className="mt-1 text-sm text-slate-400">
          应用版本信息和社区链接。
        </p>
      </div>

      <Card className="p-6">
        <SectionTitle title="OPP" />

        <div className="mt-4">
          <DataLine label="版本" value={APP_VERSION} />
          <DataLine
            label="描述"
            value="一个面向 osu! 玩家的全功能桌面工具箱"
          />
        </div>

        <div className="mt-4 flex flex-wrap gap-2 border-t border-[var(--line-subtle)] pt-4">
          <Button
            onClick={() =>
              void desktopApi.openExternal(
                "https://github.com/osuplusplus/OPP"
              )
            }
            size="sm"
            variant="secondary"
          >
            <ExternalLink className="size-4" />
            GitHub 仓库
          </Button>

          <Button
            onClick={() =>
              void desktopApi.openExternal(
                "https://github.com/osuplusplus/OPP/releases"
              )
            }
            size="sm"
            variant="secondary"
          >
            <ExternalLink className="size-4" />
            更新日志
          </Button>
        </div>
      </Card>

      <Card className="p-6">
        <SectionTitle title="社区" />

        <div className="mt-4">
          <DataLine label="QQ 群" value={COMMUNITY_GROUP_NUMBER} />
          <DataLine label="口令" value={COMMUNITY_GROUP_MESSAGE} />
        </div>

        <div className="mt-4 flex flex-wrap gap-2 border-t border-[var(--line-subtle)] pt-4">
          <Button
            onClick={() =>
              void desktopApi.openExternal(
                `https://qm.qq.com/q/${COMMUNITY_GROUP_NUMBER}`
              )
            }
            size="sm"
            variant="secondary"
          >
            <Users className="size-4" />
            加入 QQ 群
          </Button>
        </div>
      </Card>

      <div className="rounded-lg border border-white/[0.06] bg-white/[0.02] p-4">
        <p className="text-xs text-slate-400">
          OPP 是一个独立的社区项目，与 osu! 官方无关。
          <br />
          本项目遵循 MIT 许可证开源。
        </p>
      </div>
    </div>
  );
}
