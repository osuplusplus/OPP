import { AlertTriangle, CheckCircle2, Info, X, XCircle } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";

import { cn } from "../lib/cn";
import { Button } from "./ui";

export type NotificationTone = "info" | "success" | "warning" | "error";

export interface NotificationAction {
  label: string;
  onClick: () => void | Promise<void>;
}

export interface NotificationOptions {
  title: string;
  description?: string;
  tone?: NotificationTone;
  duration?: number | null;
  action?: NotificationAction;
}

interface NotificationItem extends NotificationOptions {
  id: string;
}

const toneStyles: Record<NotificationTone, { border: string; icon: string; Icon: typeof Info }> = {
  info: { border: "border-cyan-300/20", icon: "text-cyan-300", Icon: Info },
  success: { border: "border-emerald-300/20", icon: "text-emerald-300", Icon: CheckCircle2 },
  warning: { border: "border-amber-300/20", icon: "text-amber-300", Icon: AlertTriangle },
  error: { border: "border-rose-300/20", icon: "text-rose-300", Icon: XCircle },
};

let sequence = 0;
let items: NotificationItem[] = [];
const listeners = new Set<(next: NotificationItem[]) => void>();
const timers = new Map<string, number>();

function publish() {
  listeners.forEach((listener) => listener(items));
}

function dismiss(id: string) {
  const timer = timers.get(id);
  if (timer !== undefined) window.clearTimeout(timer);
  timers.delete(id);
  items = items.filter((item) => item.id !== id);
  publish();
}

function show(options: NotificationOptions) {
  const id = `notice-${Date.now()}-${sequence += 1}`;
  const item: NotificationItem = { tone: "info", duration: 5_000, ...options, id };
  const duration = item.duration ?? 5_000;
  items = [...items, item].slice(-4);
  publish();
  if (item.duration !== null && duration > 0) {
    timers.set(id, window.setTimeout(() => dismiss(id), duration));
  }
  return id;
}

type NoticeDetails = Omit<NotificationOptions, "title" | "tone"> | string | undefined;

function toneShortcut(tone: NotificationTone) {
  return (title: string, details?: NoticeDetails) => show({
    title,
    tone,
    ...(typeof details === "string" ? { description: details } : details),
  });
}

/** Imperative notification API for feature code outside the React tree. */
// A shared command API and its viewport intentionally live together as one contract.
// eslint-disable-next-line react-refresh/only-export-components
export const notification = {
  show,
  dismiss,
  info: toneShortcut("info"),
  success: toneShortcut("success"),
  warning: toneShortcut("warning"),
  error: toneShortcut("error"),
};

export function NotificationCard({
  title,
  description,
  tone = "info",
  icon,
  onClose,
  closeLabel = "关闭通知",
  action,
  headerActions,
  children,
  className,
  descriptionClassName,
}: {
  title: ReactNode;
  description?: ReactNode;
  tone?: NotificationTone;
  icon?: ReactNode;
  onClose?: () => void;
  closeLabel?: string;
  action?: NotificationAction;
  headerActions?: ReactNode;
  children?: ReactNode;
  className?: string;
  descriptionClassName?: string;
}) {
  const style = toneStyles[tone];
  const Icon = style.Icon;
  return (
    <section
      aria-atomic="true"
      className={cn("pointer-events-auto w-[min(380px,calc(100vw-2rem))] overflow-hidden rounded-xl border bg-[var(--surface-float)] shadow-[0_18px_60px_rgba(0,0,0,.32)] backdrop-blur-xl", style.border, className)}
      data-notification-tone={tone}
      role={tone === "error" ? "alert" : "status"}
    >
      <div className="flex items-start gap-3 p-4">
        <span className={cn("mt-0.5 shrink-0", style.icon)}>{icon ?? <Icon className="size-5" />}</span>
        <div className="min-w-0 flex-1">
          <h2 className="text-sm font-semibold text-white">{title}</h2>
          {description ? <p className={cn("mt-1 text-xs leading-5 text-slate-400", descriptionClassName)}>{description}</p> : null}
        </div>
        {headerActions}
        {onClose ? <button aria-label={closeLabel} className="grid size-7 shrink-0 place-items-center rounded-md text-slate-500 hover:bg-[var(--surface-interactive-hover)] hover:text-white" onClick={onClose} type="button"><X className="size-4" /></button> : null}
      </div>
      {children ? <div className="border-t border-[var(--line-subtle)] px-4 py-3">{children}</div> : null}
      {action ? <div className="flex justify-end border-t border-[var(--line-subtle)] px-4 py-3"><Button onClick={() => void action.onClick()} size="sm" variant="ghost">{action.label}</Button></div> : null}
    </section>
  );
}

export function NotificationViewport() {
  const [visible, setVisible] = useState(items);
  useEffect(() => {
    const listener = (next: NotificationItem[]) => setVisible(next);
    listeners.add(listener);
    listener(items);
    return () => { listeners.delete(listener); };
  }, []);

  if (!visible.length) return null;
  return (
    <aside aria-label="应用通知" className="pointer-events-none fixed right-6 top-[calc(var(--titlebar-height)+16px)] z-[290] flex flex-col gap-3">
      {visible.map((item) => <NotificationCard action={item.action} description={item.description} key={item.id} onClose={() => dismiss(item.id)} title={item.title} tone={item.tone} />)}
    </aside>
  );
}
