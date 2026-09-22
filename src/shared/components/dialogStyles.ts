export const appDialogStyles = {
  overlay: "fixed inset-0 z-[260] bg-black/60 backdrop-blur-sm",
  content: "fixed left-1/2 top-1/2 z-[270] flex max-h-[min(720px,calc(100dvh-2rem))] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-xl border border-[var(--line-strong)] bg-[var(--surface-float)] text-[var(--text)] shadow-[0_24px_80px_rgba(0,0,0,.38)] outline-none",
  header: "flex shrink-0 items-start gap-4 border-b border-[var(--line-subtle)] px-6 py-5",
  body: "min-h-0 flex-1 overflow-y-auto px-6 py-5",
  footer: "flex shrink-0 flex-wrap justify-end gap-2 border-t border-[var(--line-subtle)] px-6 py-4",
} as const;

