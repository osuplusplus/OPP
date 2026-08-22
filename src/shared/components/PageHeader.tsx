import type { ReactNode } from "react";
import { CircleHelp } from "lucide-react";
import { START_PAGE_ONBOARDING_EVENT } from "../lib/onboardingEvents";

export function PageHeader({
  title,
  actions,
}: {
  eyebrow?: string;
  title: string;
  /**
   * Kept optional while callers are migrated. Page-level helper copy is no
   * longer displayed beneath titles.
   */
  description?: string;
  actions?: ReactNode;
}) {
  return (
    <header className="mb-6 flex items-center justify-between gap-8 border-b border-[var(--line-subtle)] pb-4">
      <div className="flex min-w-0 items-center gap-3" data-page-guide-title="true">
        <h1 className="text-[24px] font-semibold leading-none tracking-[-0.025em] text-white">
          {title}
        </h1>
        <button
          aria-label={`查看“${title}”页面引导`}
          className="grid size-8 shrink-0 place-items-center rounded-full text-slate-500 transition-colors hover:bg-white/[0.07] hover:text-[var(--theme-primary)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--theme-primary)]"
          onClick={() => window.dispatchEvent(new Event(START_PAGE_ONBOARDING_EVENT))}
          title="查看本页引导"
          type="button"
        >
          <CircleHelp className="size-4" />
        </button>
      </div>
      {actions ? <div className="shrink-0 pb-1">{actions}</div> : null}
    </header>
  );
}
