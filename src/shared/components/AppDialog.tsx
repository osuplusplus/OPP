import * as Dialog from "@radix-ui/react-dialog";
import { X } from "lucide-react";
import type { ComponentPropsWithoutRef, ReactNode } from "react";

import { cn } from "../lib/cn";
import { appDialogStyles } from "./dialogStyles";

const widths = {
  sm: "w-[min(440px,calc(100vw-2rem))]",
  md: "w-[min(620px,calc(100vw-2rem))]",
  lg: "w-[min(760px,calc(100vw-2rem))]",
} as const;

export interface AppDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: ReactNode;
  description?: ReactNode;
  icon?: ReactNode;
  iconClassName?: string;
  children: ReactNode;
  footer?: ReactNode;
  size?: keyof typeof widths;
  closeLabel?: string;
  closeDisabled?: boolean;
  hideClose?: boolean;
  contentClassName?: string;
  bodyClassName?: string;
  overlayProps?: ComponentPropsWithoutRef<typeof Dialog.Overlay>;
  overlayTestId?: string;
  onCloseAutoFocus?: ComponentPropsWithoutRef<typeof Dialog.Content>["onCloseAutoFocus"];
}

/** Standard modal contract for confirmation and focused application workflows. */
export function AppDialog({
  open,
  onOpenChange,
  title,
  description,
  icon,
  iconClassName,
  children,
  footer,
  size = "md",
  closeLabel = "关闭",
  closeDisabled = false,
  hideClose = false,
  contentClassName,
  bodyClassName,
  overlayProps,
  overlayTestId,
  onCloseAutoFocus,
}: AppDialogProps) {
  const { className: overlayClassName, ...restOverlayProps } = overlayProps ?? {};
  return (
    <Dialog.Root onOpenChange={onOpenChange} open={open}>
      <Dialog.Portal>
        <Dialog.Overlay
          className={cn(appDialogStyles.overlay, overlayClassName)}
          data-testid={overlayTestId}
          {...restOverlayProps}
        />
        <Dialog.Content
          className={cn(appDialogStyles.content, widths[size], contentClassName)}
          data-dialog-layout="standard"
          onCloseAutoFocus={onCloseAutoFocus}
        >
          <header className={appDialogStyles.header}>
            {icon ? <span className={cn("grid size-10 shrink-0 place-items-center rounded-lg bg-[var(--theme-primary-muted)] text-[var(--theme-primary-light)]", iconClassName)}>{icon}</span> : null}
            <div className="min-w-0 flex-1">
              <Dialog.Title className="text-lg font-semibold text-white">{title}</Dialog.Title>
              {description ? <Dialog.Description className="mt-1.5 text-sm leading-6 text-slate-400">{description}</Dialog.Description> : null}
            </div>
            {!hideClose ? (
              <Dialog.Close asChild>
                <button
                  aria-label={closeLabel}
                  className="grid size-9 shrink-0 place-items-center rounded-lg text-slate-500 transition-colors hover:bg-[var(--surface-interactive-hover)] hover:text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--theme-primary)] disabled:pointer-events-none disabled:opacity-40"
                  disabled={closeDisabled}
                  type="button"
                >
                  <X className="size-5" />
                </button>
              </Dialog.Close>
            ) : null}
          </header>
          <div className={cn(appDialogStyles.body, bodyClassName)}>{children}</div>
          {footer ? <footer className={appDialogStyles.footer}>{footer}</footer> : null}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
