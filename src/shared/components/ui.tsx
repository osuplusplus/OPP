import { forwardRef } from "react";
import type { ButtonHTMLAttributes, HTMLAttributes, InputHTMLAttributes, ReactElement, ReactNode, SelectHTMLAttributes } from "react";
import * as TooltipPrimitive from "@radix-ui/react-tooltip";
import { cva, type VariantProps } from "class-variance-authority";
import { AlertCircle, CircleHelp, LoaderCircle } from "lucide-react";
import { cn } from "../lib/cn";

const buttonVariants = cva(
  "opp-action inline-flex cursor-pointer items-center justify-center gap-2 font-semibold outline-none focus-visible:ring-2 focus-visible:ring-[var(--theme-primary)] focus-visible:ring-offset-2 focus-visible:ring-offset-[var(--surface)] disabled:pointer-events-none disabled:opacity-45",
  {
    variants: {
      variant: {
        primary:
          "border border-transparent bg-[var(--theme-primary)] px-4 py-2.5 text-[var(--on-primary)] shadow-[0_8px_20px_var(--theme-primary-glow)] hover:bg-[var(--theme-primary-strong)]",
        secondary:
          "border border-[var(--line-subtle)] bg-[var(--surface-interactive)] px-4 py-2.5 text-slate-100 hover:border-[var(--line-strong)] hover:bg-[var(--surface-interactive-hover)]",
        ghost:
          "px-3 py-2 text-slate-300 hover:bg-[var(--surface-interactive-hover)] hover:text-white",
        danger:
          "border border-rose-400/25 bg-rose-400/10 px-4 py-2.5 text-rose-200 hover:bg-rose-400/18",
      },
      size: {
        md: "min-h-10 text-sm",
        sm: "min-h-8 text-xs",
        icon: "size-9 p-0",
      },
    },
    defaultVariants: { variant: "secondary", size: "md" },
  },
);

export interface ButtonProps
  extends ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  loading?: boolean;
}

export type InputProps = InputHTMLAttributes<HTMLInputElement>;

// 复用旧的 opp-input 样式契约，公共组件迁移不会迫使现有页面同步修改。
export const Input = forwardRef<HTMLInputElement, InputProps>(({ className, ...props }, ref) => (
  <input className={cn("opp-input", className)} data-slot="input" ref={ref} {...props} />
));
Input.displayName = "Input";

export type SelectProps = SelectHTMLAttributes<HTMLSelectElement>;

// Select 与 Input 保持相同的尺寸、焦点环和主题适配。
export const Select = forwardRef<HTMLSelectElement, SelectProps>(({ className, ...props }, ref) => (
  <select className={cn("opp-input", className)} data-slot="select" ref={ref} {...props} />
));
Select.displayName = "Select";

export function Button({
  className,
  variant,
  size,
  loading,
  children,
  disabled,
  ...props
}: ButtonProps) {
  return (
    <button
      className={cn(buttonVariants({ variant, size }), className)}
      data-slot="button"
      disabled={disabled || loading}
      {...props}
    >
      {loading ? <LoaderCircle className="size-4 animate-spin" /> : null}
      {children}
    </button>
  );
}

export interface CardProps extends HTMLAttributes<HTMLDivElement> {
  unstyled?: boolean;
}

export function Card({
  className,
  unstyled = false,
  ...props
}: CardProps) {
  return (
    <div
      className={cn(
        // 新卡片可以完全使用 Tailwind；旧页面继续继承 opp-section，避免批量迁移。
        !unstyled && "opp-section",
        className,
      )}
      data-slot="card"
      {...props}
    />
  );
}

export interface BadgeProps {
  children: ReactNode;
  tone?: "neutral" | "pink" | "cyan" | "warning" | "success";
  className?: string;
}

export function Badge({
  children,
  tone = "neutral",
  className,
}: BadgeProps) {
  const tones = {
    neutral: "border-white/10 bg-white/[0.055] text-slate-200",
    pink: "border-pink-400/20 bg-pink-400/10 text-pink-200",
    cyan: "border-[var(--theme-primary-soft)] bg-[var(--theme-primary-muted)] text-[var(--theme-primary-light)]",
    warning: "border-amber-300/20 bg-amber-300/10 text-amber-100",
    success: "border-emerald-300/20 bg-emerald-300/10 text-emerald-100",
  };
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-md border px-2 py-1 text-[10px] font-bold uppercase tracking-[0.08em]",
        tones[tone],
        className,
      )}
      data-slot="badge"
    >
      {children}
    </span>
  );
}

export function Skeleton({ className }: { className?: string }) {
  return (
    <div
      className={cn(
        "animate-pulse bg-gradient-to-r from-white/[0.04] via-white/[0.09] to-white/[0.04] bg-[length:220%_100%]",
        className,
      )}
    />
  );
}

export function EmptyState({
  icon,
  title,
  description,
  action,
}: {
  icon?: ReactNode;
  title: string;
  description: string;
  action?: ReactNode;
}) {
  return (
    <Card className="grid min-h-64 place-items-center p-8 text-center">
      <div className="max-w-md">
        <div className="mx-auto mb-4 grid size-11 place-items-center border-y border-[var(--line-strong)] text-[var(--theme-primary)]">
          {icon ?? <AlertCircle className="size-5" />}
        </div>
        <h3 className="text-base font-semibold text-white">{title}</h3>
        <p className="mt-2 text-sm leading-6 text-slate-300">{description}</p>
        {action ? <div className="mt-5">{action}</div> : null}
      </div>
    </Card>
  );
}

export function DataLine({
  label,
  value,
}: {
  label: string;
  value: ReactNode;
}) {
  return (
    <div className="flex min-h-11 items-center justify-between gap-5 border-b border-white/[0.06] py-2.5 last:border-b-0">
      <span className="text-sm text-slate-300">{label}</span>
      <span className="text-right text-sm font-medium text-slate-200">
        {value ?? "—"}
      </span>
    </div>
  );
}

export function SectionTitle({
  title,
  description,
}: {
  eyebrow?: string;
  title: string;
  description?: string;
}) {
  return (
    <div>
      <div className="flex items-center gap-2">
        <h2 className="text-base font-semibold tracking-tight text-white">{title}</h2>
        {description ? <InfoTip text={description} /> : null}
      </div>
    </div>
  );
}

export interface TooltipProps {
  children: ReactElement;
  content: ReactNode;
  side?: "top" | "right" | "bottom" | "left";
}

export function Tooltip({
  children,
  content,
  side = "top",
}: TooltipProps) {
  return (
    <TooltipPrimitive.Provider delayDuration={300}>
      <TooltipPrimitive.Root>
        <TooltipPrimitive.Trigger asChild>{children}</TooltipPrimitive.Trigger>
        <TooltipPrimitive.Portal>
          <TooltipPrimitive.Content
            className="opp-floating z-[200] max-w-xs px-3 py-2 text-xs leading-5 text-slate-200"
            data-slot="tooltip-content"
            side={side}
            sideOffset={7}
          >
            {content}
            <TooltipPrimitive.Arrow className="fill-[var(--surface-float)]" />
          </TooltipPrimitive.Content>
        </TooltipPrimitive.Portal>
      </TooltipPrimitive.Root>
    </TooltipPrimitive.Provider>
  );
}

export function InfoTip({ text }: { text: string }) {
  return (
    <Tooltip content={text}>
      <button aria-label={text} className="opp-action inline-grid size-4 shrink-0 cursor-help place-items-center rounded-full border border-current/35 text-slate-500 hover:text-slate-200" type="button"><CircleHelp className="size-3" /></button>
    </Tooltip>
  );
}
