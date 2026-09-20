import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";
import { Copy, Minus, Square, X } from "lucide-react";
import { isTauri } from "../lib/tauri";

async function windowAction(action: "minimize" | "maximize" | "close") {
  if (!isTauri()) return;
  const appWindow = getCurrentWindow();
  if (action === "minimize") await appWindow.minimize();
  if (action === "maximize") await appWindow.toggleMaximize();
  if (action === "close") window.dispatchEvent(new Event("opp:request-close"));
}

export function TitleBar() {
  const [maximized, setMaximized] = useState(false);
  useEffect(() => {
    if (!isTauri()) return;
    const appWindow = getCurrentWindow();
    let disposed = false;
    let unlisten: (() => void) | undefined;
    const sync = async () => {
      const next = await appWindow.isMaximized();
      if (!disposed) setMaximized(next);
    };
    void appWindow.onResized(() => { void sync().catch(() => undefined); }).then((off) => {
      if (disposed) off(); else unlisten = off;
    }).catch(() => undefined);
    void sync().catch(() => undefined);
    return () => { disposed = true; unlisten?.(); };
  }, []);
  return (
    <div
      className="theme-titlebar fixed inset-x-0 top-0 z-50 flex h-[var(--titlebar-height)] items-center border-b border-white/[0.08] bg-[var(--surface-chrome)] pl-4"
      data-tauri-drag-region
    >
      <div
        className="flex items-center gap-3 text-sm font-semibold tracking-wide text-slate-200"
        data-tauri-drag-region
      >
        <img alt="" className="opp-title-mark pointer-events-none h-8 w-7 object-contain" draggable={false} src="/03.png" />
        <span className="pointer-events-none">OSU! Plus Plus</span>
      </div>
      <div className="ml-auto flex h-full items-center">
        <button
          aria-label="最小化"
          className="opp-window-control grid h-full min-w-11 place-items-center text-slate-500 hover:bg-[var(--surface-interactive-hover)] hover:text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-[var(--theme-primary)]"
          onClick={() => windowAction("minimize")}
          type="button"
        >
          <Minus className="size-4" />
        </button>
        <button
          aria-label={maximized ? "还原窗口" : "最大化"}
          className="opp-window-control grid h-full min-w-11 place-items-center text-slate-500 hover:bg-[var(--surface-interactive-hover)] hover:text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-[var(--theme-primary)]"
          onClick={() => windowAction("maximize")}
          type="button"
        >
          {maximized ? <Copy className="size-3.5" /> : <Square className="size-3.5" />}
        </button>
        <button
          aria-label="关闭"
          className="opp-window-control grid h-full min-w-11 place-items-center text-slate-500 hover:bg-rose-600 hover:text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-rose-400"
          onClick={() => windowAction("close")}
          type="button"
        >
          <X className="size-4" />
        </button>
      </div>
    </div>
  );
}
