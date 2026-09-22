import * as Dialog from "@radix-ui/react-dialog";
import { useContext, type ReactNode } from "react";
import { useNavigate } from "react-router-dom";
import { RouteOverlayContext } from "../../app/routeOverlay";

export function RouteDialog({ title, children }: { title: string; children: ReactNode }) {
  const closeOverlay = useContext(RouteOverlayContext);
  const navigate = useNavigate();
  return <Dialog.Root open onOpenChange={(open) => { if (!open) { if (closeOverlay) closeOverlay(); else navigate("/online/beatmaps", { replace: true }); } }}>
    <Dialog.Portal>
      <Dialog.Overlay className="fixed inset-0 z-[70] bg-black/50 backdrop-blur-sm" />
      <Dialog.Content data-dialog-layout="route" aria-describedby={undefined} className="fixed left-1/2 top-1/2 z-[71] flex h-[90dvh] w-[calc(100vw-32px)] max-w-6xl -translate-x-1/2 -translate-y-1/2 overflow-hidden rounded-xl border border-[var(--line-subtle)] bg-[var(--surface)] shadow-2xl">
        <Dialog.Title className="sr-only">{title}</Dialog.Title>
        {children}
      </Dialog.Content>
    </Dialog.Portal>
  </Dialog.Root>;
}
