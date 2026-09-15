import { useEffect, useRef, useState } from "react";
import { ChevronDown, Loader2, Play } from "lucide-react";
import { desktopApi } from "../shared/lib/tauri";
import { cn } from "../shared/lib/cn";
import { rulesetLabels } from "../shared/lib/format";
import type { GameStatusSnapshot, OsuClient } from "../shared/types/osu";
import { useMode } from "./ModeContext";

const clients = [{ value: "stable", label: "Stable" }, { value: "lazer", label: "Lazer" }] as const;

export function GameLauncher() {
  const { client, ruleset } = useMode();
  const [status, setStatus] = useState<GameStatusSnapshot | null>(null);
  const [statusError, setStatusError] = useState(false);
  const [starting, setStarting] = useState<OsuClient | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  const container = useRef<HTMLDivElement>(null);
  const toggle = useRef<HTMLButtonElement>(null);
  const running = status?.clients.filter((item) => item.running) ?? [];
  const selectedRunning = running.some((item) => item.client === client);
  const clientLabel = clients.find((item) => item.value === client)!.label;

  useEffect(() => {
    let disposed = false;
    let receivedEvent = false;
    let off: (() => void) | undefined;
    const update = (next: GameStatusSnapshot) => {
      if (!disposed) { setStatus(next); setStatusError(false); }
    };
    void desktopApi.onGameStatusChanged((next) => {
      receivedEvent = true;
      update(next);
    }).then((unlisten) => {
      if (disposed) unlisten();
      else off = unlisten;
    }).catch(() => { if (!disposed) setStatusError(true); });
    void desktopApi.getGameStatus().then((next) => {
      if (!receivedEvent) update(next);
    }).catch(() => { if (!disposed && !receivedEvent) setStatusError(true); });
    return () => { disposed = true; off?.(); };
  }, []);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!container.current?.contains(event.target as Node)) setOpen(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") { setOpen(false); toggle.current?.focus(); }
    };
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  const startGame = async (target: OsuClient) => {
    if (starting || running.some((item) => item.client === target)) return;
    setStarting(target);
    setOpen(false);
    setError(null);
    try {
      await desktopApi.startGameSession(ruleset, target);
    } catch (caught) {
      setError((caught as { message?: string } | null)?.message ?? "启动失败，请在设置中检查游戏目录。");
    } finally { setStarting(null); }
  };

  return (
    <div className="relative shrink-0 border-t border-[var(--line-subtle)] pt-3" data-onboarding="start-game" ref={container}>
      <div aria-live="polite" className="mb-2 flex min-h-4 items-center gap-2 px-1 text-[11px] text-slate-400" role="status">
        <span aria-hidden="true" className={cn("size-1.5 shrink-0 rounded-full", statusError ? "bg-amber-400" : running.length ? "bg-emerald-400" : "bg-slate-500")} />
        <span className="truncate" title={running.map((item) => item.executable).filter(Boolean).join("\n")}>
          {statusError ? "运行状态暂不可用" : running.length ? `${running.map((item) => item.client === "stable" ? "Stable" : "Lazer").join(" + ")} 运行中` : status ? "游戏未运行" : "正在检测游戏…"}
        </span>
      </div>
      <div className="flex rounded-lg bg-[var(--theme-primary)] text-[var(--on-primary)]">
        <button
          className="flex min-h-12 min-w-0 flex-1 items-center gap-2.5 rounded-l-lg px-3 py-2 text-left transition-colors hover:bg-[var(--theme-primary-strong)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-current disabled:opacity-60"
          disabled={starting !== null || selectedRunning}
          onClick={() => void startGame(client)} type="button"
        >
          {starting ? <Loader2 className="size-4 shrink-0 animate-spin" /> : <Play className="size-4 shrink-0" />}
          <span className="min-w-0"><span className="block text-xs font-semibold">{starting ? "正在启动…" : selectedRunning ? "osu! 运行中" : "启动 osu!"}</span><span className="block truncate text-[10px] opacity-75">{clientLabel} · {rulesetLabels[ruleset]}</span></span>
        </button>
        <button aria-controls="game-launch-options" aria-expanded={open} aria-label="选择其他客户端启动" className="grid w-9 shrink-0 place-items-center rounded-r-lg border-l border-black/10 transition-colors hover:bg-[var(--theme-primary-strong)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-current disabled:opacity-60" disabled={starting !== null} onClick={() => setOpen((value) => !value)} ref={toggle} type="button">
          <ChevronDown className={cn("size-3.5 transition-transform", open && "rotate-180")} />
        </button>
      </div>
      {open ? <div aria-label="启动客户端" className="absolute bottom-[calc(100%+8px)] left-0 z-50 w-full rounded-lg border border-[var(--line-subtle)] bg-[var(--surface-panel-strong)] p-2 shadow-2xl" id="game-launch-options">
        {clients.map((item) => {
          const isRunning = running.some((entry) => entry.client === item.value);
          return <button aria-label={`${isRunning ? "运行中" : "启动"} osu! ${item.label}`} className="flex min-h-10 w-full items-center justify-between gap-2 rounded-md px-2 py-2 text-left text-xs text-slate-300 hover:bg-white/[0.06] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--theme-primary)] disabled:opacity-50" disabled={isRunning} key={item.value} onClick={() => void startGame(item.value)} type="button"><span>osu! {item.label}</span><span className="text-[10px] text-slate-400">{isRunning ? "运行中" : "启动"}</span></button>;
        })}
      </div> : null}
      {error ? <p className="mt-2 break-words px-1 text-[11px] text-rose-300" role="alert">{error}</p> : null}
    </div>
  );
}
