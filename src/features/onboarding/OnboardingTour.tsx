import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { ArrowLeft, ArrowRight, Check, X } from "lucide-react";
import { Button } from "../../shared/components/ui";
import { onboardingSteps, type OnboardingStep } from "./tourContent";

interface TargetBox {
  left: number;
  top: number;
  width: number;
  height: number;
}

interface PanelPosition {
  left: number;
  top: number;
}

function clamp(value: number, minimum: number, maximum: number) {
  return Math.min(Math.max(value, minimum), Math.max(minimum, maximum));
}

function findStepTarget(step: OnboardingStep) {
  if (step.target) return document.querySelector<HTMLElement>(step.target);
  if (!step.targetText) return null;
  const selector = step.targetTextSelector ?? "h2, h3, summary, button, [role='group']";
  const candidates = Array.from(document.querySelectorAll<HTMLElement>(selector));
  const text = step.targetText.trim();
  return candidates.find((candidate) => candidate.textContent?.trim() === text)
    ?? candidates.find((candidate) => candidate.textContent?.includes(text))
    ?? null;
}

function expandForStep(step: OnboardingStep) {
  const target = findStepTarget(step);
  const expandable = step.expandSelector
    ? document.querySelector<HTMLElement>(step.expandSelector)
    : step.expandTarget
      ? target
      : null;
  if (!expandable) return () => undefined;

  const details = expandable instanceof HTMLDetailsElement
    ? expandable
    : expandable.closest("details");
  if (details && !details.open) {
    details.open = true;
    return () => { details.open = false; };
  }

  if (expandable instanceof HTMLButtonElement) {
    const expanded = expandable.getAttribute("aria-expanded");
    if (expanded === "false") {
      expandable.click();
      return () => {
        if (expandable.getAttribute("aria-expanded") === "true") expandable.click();
      };
    }
  }
  return () => undefined;
}

export function OnboardingTour({
  eyebrow = "新手引导",
  reduceMotion,
  onClose,
  steps = onboardingSteps,
}: {
  eyebrow?: string;
  reduceMotion: boolean;
  onClose: () => void;
  steps?: OnboardingStep[];
}) {
  const [stepIndex, setStepIndex] = useState(0);
  const [targetBox, setTargetBox] = useState<TargetBox | null>(null);
  const [panelPosition, setPanelPosition] = useState<PanelPosition | null>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const previousFocus = useRef<HTMLElement | null>(null);
  const step = steps[stepIndex];

  useEffect(() => {
    previousFocus.current = document.activeElement as HTMLElement | null;
    return () => previousFocus.current?.focus();
  }, []);

  useLayoutEffect(() => {
    let frame = 0;
    let settleTimer = 0;
    const restoreExpandedState = expandForStep(step);

    const updatePosition = () => {
      const target = findStepTarget(step);
      const panel = panelRef.current;
      const panelWidth = panel?.offsetWidth ?? Math.min(400, window.innerWidth - 32);
      const panelHeight = panel?.offsetHeight ?? 280;
      const margin = 16;
      const gap = 18;

      if (!target) {
        setTargetBox(null);
        setPanelPosition({
          left: Math.max(margin, (window.innerWidth - panelWidth) / 2),
          top: Math.max(margin, (window.innerHeight - panelHeight) / 2),
        });
        return;
      }

      const rect = target.getBoundingClientRect();
      const padding = 6;
      const box = {
        left: clamp(rect.left - padding, 4, window.innerWidth - 5),
        top: clamp(rect.top - padding, 4, window.innerHeight - 5),
        width: 0,
        height: 0,
      };
      box.width = Math.max(1, Math.min(window.innerWidth - box.left - 4, rect.width + padding * 2));
      box.height = Math.max(1, Math.min(window.innerHeight - box.top - 4, rect.height + padding * 2));
      setTargetBox(box);

      const spaceRight = window.innerWidth - (box.left + box.width);
      const spaceLeft = box.left;
      let left = spaceRight >= panelWidth + gap
        ? box.left + box.width + gap
        : spaceLeft >= panelWidth + gap
          ? box.left - panelWidth - gap
          : clamp(box.left, margin, window.innerWidth - panelWidth - margin);
      let top = clamp(box.top, margin, window.innerHeight - panelHeight - margin);

      if (spaceRight < panelWidth + gap && spaceLeft < panelWidth + gap) {
        const below = box.top + box.height + gap;
        const above = box.top - panelHeight - gap;
        top = below + panelHeight <= window.innerHeight - margin
          ? below
          : Math.max(margin, above);
        left = clamp(box.left + box.width / 2 - panelWidth / 2, margin, window.innerWidth - panelWidth - margin);
      }
      setPanelPosition({ left, top });
    };

    const target = findStepTarget(step);
    target?.scrollIntoView({
      block: "nearest",
      behavior: reduceMotion ? "auto" : "smooth",
    });
    frame = window.requestAnimationFrame(updatePosition);
    settleTimer = window.setTimeout(updatePosition, reduceMotion ? 0 : 260);
    window.addEventListener("resize", updatePosition);
    document.addEventListener("scroll", updatePosition, true);
    return () => {
      window.cancelAnimationFrame(frame);
      window.clearTimeout(settleTimer);
      window.removeEventListener("resize", updatePosition);
      document.removeEventListener("scroll", updatePosition, true);
      restoreExpandedState();
    };
  }, [reduceMotion, step]);

  useEffect(() => {
    const panel = panelRef.current;
    panel?.focus();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
        return;
      }
      if (event.key !== "Tab" || !panel) return;
      const focusable = Array.from(
        panel.querySelectorAll<HTMLElement>('button:not([disabled]), [href], [tabindex]:not([tabindex="-1"])'),
      );
      if (!focusable.length) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [onClose, stepIndex]);

  const finalStep = stepIndex === steps.length - 1;
  const goNext = () => finalStep ? onClose() : setStepIndex((value) => value + 1);

  return (
    <div className="fixed inset-0 z-[260]" data-testid="onboarding-tour">
      <div aria-hidden="true" className={`absolute inset-0 ${targetBox ? "" : "bg-black/65"}`} />
      {targetBox ? (
        <div
          aria-hidden="true"
          className="pointer-events-none fixed rounded-xl border-2 border-[var(--theme-primary)] shadow-[0_0_0_9999px_rgba(2,6,15,0.66),0_0_28px_var(--theme-primary-glow)]"
          style={{
            height: targetBox.height,
            left: targetBox.left,
            top: targetBox.top,
            transition: reduceMotion ? "none" : "all 180ms ease",
            width: targetBox.width,
          }}
        />
      ) : null}
      <div
        aria-describedby="onboarding-description"
        aria-labelledby="onboarding-title"
        aria-modal="true"
        className="opp-onboarding-dialog fixed border border-[var(--line-strong)] bg-[var(--surface-float)] shadow-2xl"
        ref={panelRef}
        role="dialog"
        style={{
          left: panelPosition?.left ?? 16,
          opacity: panelPosition ? 1 : 0,
          top: panelPosition?.top ?? 16,
          transition: reduceMotion ? "none" : "left 180ms ease, top 180ms ease, opacity 120ms ease",
        }}
        tabIndex={-1}
      >
        <div className="flex items-start justify-between gap-4">
          <div>
            <p className="text-xs font-semibold uppercase tracking-[0.16em] text-[var(--theme-primary)]">
              {eyebrow} · {stepIndex + 1}/{steps.length}
            </p>
            <h2 className="mt-2 text-xl font-semibold text-white" id="onboarding-title">{step.title}</h2>
          </div>
          <button
            aria-label="关闭新手引导"
            className="grid size-9 shrink-0 place-items-center rounded-lg text-slate-400 hover:bg-white/[0.07] hover:text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--theme-primary)]"
            onClick={onClose}
            type="button"
          >
            <X className="size-4" />
          </button>
        </div>
        <p className="mt-4 text-sm leading-6 text-slate-300" id="onboarding-description">{step.description}</p>
        {step.example ? (
          <div className="mt-4 rounded-lg border border-[var(--theme-primary-soft)] bg-[var(--theme-primary-muted)] px-3.5 py-3">
            <p className="text-xs font-semibold text-[var(--theme-primary-light)]">操作示例</p>
            <p className="mt-1 text-xs leading-5 text-slate-300">{step.example}</p>
          </div>
        ) : null}
        <div aria-hidden="true" className="mt-5 flex gap-1">
          {steps.map((item, index) => (
            <span
              className={`h-1 flex-1 rounded-full ${index <= stepIndex ? "bg-[var(--theme-primary)]" : "bg-white/[0.1]"}`}
              key={item.title}
            />
          ))}
        </div>
        <div className="mt-5 flex items-center justify-between gap-3">
          <Button onClick={onClose} size="sm" variant="ghost">跳过引导</Button>
          <div className="flex gap-2">
            {stepIndex > 0 ? (
              <Button onClick={() => setStepIndex((value) => value - 1)} size="sm" variant="secondary">
                <ArrowLeft className="size-4" />上一步
              </Button>
            ) : null}
            <Button onClick={goNext} size="sm" variant="primary">
              {finalStep ? <><Check className="size-4" />完成</> : <>下一步<ArrowRight className="size-4" /></>}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
