/** Consume a wheel delta locally, then pass any remainder to the owning list. */
export function scrollWheel(event: WheelEvent, local: HTMLElement, owner: HTMLElement | null) {
  if (event.ctrlKey || event.shiftKey || Math.abs(event.deltaX) > Math.abs(event.deltaY) || !event.deltaY) return;
  let remaining = event.deltaY * (event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? local.clientHeight : 1);
  for (const node of [local, owner]) {
    if (!node) continue;
    const before = node.scrollTop;
    const next = Math.max(0, Math.min(node.scrollHeight - node.clientHeight, before + remaining));
    node.scrollTop = next;
    remaining -= next - before;
    if (Math.abs(remaining) < 1) break;
  }
  event.preventDefault();
}
