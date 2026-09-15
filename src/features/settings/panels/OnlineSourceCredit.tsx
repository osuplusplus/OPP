import { ExternalLink } from "lucide-react";
import { desktopApi } from "../../../shared/lib/tauri";

const SAYOBOT_WEBSITE = "https://osu.sayobot.cn";

export function OnlineSourceCredit() {
  return (
    <div className="mt-4 flex flex-wrap items-center justify-between gap-3 rounded-xl border border-white/[0.1] bg-white/[0.035] p-4">
      <div>
        <p className="text-sm text-slate-300">
          在线背景与“小夜”镜像下载来源：
          <button
            className="ml-1 font-semibold text-[var(--theme-primary-light)] underline decoration-white/20 underline-offset-4 transition-colors hover:text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--theme-primary-soft)]"
            onClick={() => void desktopApi.openExternal(SAYOBOT_WEBSITE)}
            type="button"
          >
            小夜官网
          </button>
        </p>
        <p className="mt-1 text-xs leading-5 text-slate-500">
          感谢小夜长期为 osu! 社区提供公益谱面服务。
        </p>
      </div>
      <button
        aria-label="打开小夜官网"
        className="grid size-9 shrink-0 place-items-center rounded-lg border border-white/[0.1] text-slate-400 transition-colors hover:border-white/20 hover:bg-white/[0.06] hover:text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--theme-primary-soft)]"
        onClick={() => void desktopApi.openExternal(SAYOBOT_WEBSITE)}
        title="打开小夜官网"
        type="button"
      >
        <ExternalLink className="size-4" />
      </button>
    </div>
  );
}
