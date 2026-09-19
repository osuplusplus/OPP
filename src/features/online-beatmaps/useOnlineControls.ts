import { useEffect, type RefObject } from "react";
import type { OnlineView } from "./stageModel";

export function useOnlineControls({ workspaceRef, view, onBack, onHome, onStep, onPreview }: {
  workspaceRef: RefObject<HTMLElement | null>; view: OnlineView;
  onBack: () => void; onHome: () => void; onStep: (direction: number) => void; onPreview: () => void;
}) {
  useEffect(() => {
    const modalOpen = () => !!document.querySelector('[role="dialog"], [role="alertdialog"], .online-visual-preview');
    const keydown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.isComposing || event.keyCode === 229 || modalOpen()) return;
      const target = event.target instanceof Element ? event.target : null;
      const editing = !!target?.closest('input, textarea, select, [contenteditable="true"]');
      const interactive = !!target?.closest('button, a, summary, [role="tab"], .online-difficulty-popover');
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k" || (!editing && event.key === "/")) {
        event.preventDefault(); const input = workspaceRef.current?.querySelector<HTMLInputElement>(".online-search input"); input?.focus(); input?.select();
      } else if (event.altKey && event.key === "ArrowLeft") {
        event.preventDefault(); if (!event.repeat) onBack();
      } else if (event.altKey && event.key === "Home") {
        event.preventDefault(); onHome();
      } else if (!editing && !event.ctrlKey && !event.metaKey && !event.altKey && view === "stage" && ["j", "k"].includes(event.key.toLowerCase())) {
        event.preventDefault(); onStep(event.key.toLowerCase() === "j" ? 1 : -1);
      } else if (!editing && !event.ctrlKey && !event.metaKey && !event.altKey && !interactive) {
        if (event.key === "Escape" && !document.querySelector('.online-difficulty-popover, .online-batch-menu[open], .online-song-more-panel')) {
          event.preventDefault(); if (!event.repeat) onBack();
        } else if (view === "stage" && event.code === "Space") {
          event.preventDefault(); if (!event.repeat) onPreview();
        } else if (event.key === "ArrowDown") {
          const card = workspaceRef.current?.querySelector<HTMLButtonElement>("[data-result-index]");
          if (card) { event.preventDefault(); card.focus({ preventScroll: true }); }
        }
      }
    };
    // Cancel the browser's native history action on both edges of XBUTTON1.
    const sideDown = (event: MouseEvent) => { if (event.button === 3) event.preventDefault(); };
    const sideUp = (event: MouseEvent) => {
      if (event.button !== 3) return;
      event.preventDefault(); if (!modalOpen()) onBack();
    };
    const auxiliary = (event: MouseEvent) => { if (event.button === 3) event.preventDefault(); };
    document.addEventListener("keydown", keydown);
    window.addEventListener("mousedown", sideDown, true);
    window.addEventListener("mouseup", sideUp, true);
    window.addEventListener("auxclick", auxiliary, true);
    return () => {
      document.removeEventListener("keydown", keydown);
      window.removeEventListener("mousedown", sideDown, true);
      window.removeEventListener("mouseup", sideUp, true);
      window.removeEventListener("auxclick", auxiliary, true);
    };
  }, [workspaceRef, view, onBack, onHome, onStep, onPreview]);
}
