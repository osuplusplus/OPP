import * as Dialog from "@radix-ui/react-dialog";
import { useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { X } from "lucide-react";

export function StudioDropdown({ open, onOpenChange, trigger, title, description, closeLabel, children }: {
  open: boolean; onOpenChange: (value: boolean) => void; trigger: ReactNode;
  title: string; description: string; closeLabel: string; children: ReactNode;
}) {
  const anchor = useRef<HTMLButtonElement>(null);
  const [style, setStyle] = useState<CSSProperties>({ visibility: "hidden" });
  useLayoutEffect(() => {
    if (!open) return;
    const position = () => {
      const rect = anchor.current?.getBoundingClientRect();
      if (!rect) return;
      const width = Math.min(460, window.innerWidth - 24);
      const top = rect.bottom + 8;
      setStyle({ width, top, left: Math.max(12, Math.min(rect.right - width, window.innerWidth - width - 12)), maxHeight: Math.max(100, window.innerHeight - top - 12) });
    };
    position();
    const observer = new ResizeObserver(position);
    if (anchor.current) observer.observe(anchor.current);
    window.addEventListener("resize", position);
    document.addEventListener("scroll", position, true);
    return () => { observer.disconnect(); window.removeEventListener("resize", position); document.removeEventListener("scroll", position, true); };
  }, [open]);
  return <Dialog.Root open={open} onOpenChange={onOpenChange}>
    <Dialog.Trigger ref={anchor} className="studio-dropdown-trigger">{trigger}</Dialog.Trigger>
    <Dialog.Portal><Dialog.Overlay className="studio-library-overlay" />
      <Dialog.Content className="studio-library" style={style} data-dialog-layout="popover">
        <header><Dialog.Title>{title}</Dialog.Title><Dialog.Close aria-label={closeLabel}><X /></Dialog.Close></header>
        <Dialog.Description>{description}</Dialog.Description>
        {children}
      </Dialog.Content>
    </Dialog.Portal>
  </Dialog.Root>;
}
