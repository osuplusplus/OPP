import { useEffect, useRef } from "react";

/** Keep scrollbar geometry stable; only reveal the thumb during interaction. */
export function useQuietScrollbar() {
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => () => { if (timer.current) clearTimeout(timer.current); }, []);
  return (event: { currentTarget: HTMLElement }) => {
    const node = event.currentTarget;
    node.dataset.scrolling = "true";
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => { delete node.dataset.scrolling; }, 800);
  };
}
