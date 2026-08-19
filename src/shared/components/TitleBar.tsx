import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, Square, X } from "lucide-react";
import { isTauri } from "../lib/tauri";

async function windowAction(action: "minimize" | "maximize" | "close") {
  if (!isTauri()) return;
  const appWindow = getCurrentWindow();
  if (action === "minimize") await appWindow.minimize();
  if (action === "maximize") await appWindow.toggleMaximize();
  if (action === "close") window.dispatchEvent(new Event("opp:request-close"));
}

export function TitleBar() {
  return (
    <div
      className="theme-titlebar fixed inset-x-0 top-0 z-50 flex h-11 items-center border-b border-white/[0.08] bg-[var(--surface-chrome)] pl-4"
      data-tauri-drag-region
    >
      <div
        className="flex items-center gap-2.5 text-xs font-semibold tracking-wide text-slate-200"
        data-tauri-drag-region
      >
        <img alt="" className="opp-title-mark size-5 rounded-md" src="/07.png" />
        OPP
      </div>
      <div className="ml-auto flex h-full items-center">
        <button
          aria-label="最小化"
          className="opp-window-control grid min-w-11 place-items-center text-slate-500 hover:bg-[var(--surface-interactive-hover)] hover:text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-[var(--theme-primary)]"
          onClick={() => windowAction("minimize")}
          type="button"
        >
          <Minus className="size-4" />
        </button>
        <button
          aria-label="最大化"
          className="opp-window-control grid min-w-11 place-items-center text-slate-500 hover:bg-[var(--surface-interactive-hover)] hover:text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-[var(--theme-primary)]"
          onClick={() => windowAction("maximize")}
          type="button"
        >
          <Square className="size-3.5" />
        </button>
        <button
          aria-label="关闭"
          className="opp-window-control grid min-w-11 place-items-center text-slate-500 hover:bg-rose-600 hover:text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-rose-400"
          onClick={() => windowAction("close")}
          type="button"
        >
          <X className="size-4" />
        </button>
      </div>
    </div>
  );
}
