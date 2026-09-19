/** One request at a time. Hidden windows retain business tasks, not UI polling. */
export function visiblePoll(poll: () => Promise<unknown>, interval: number) {
  let disposed = false;
  let running = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const visible = () => document.visibilityState !== "hidden";
  const run = async () => {
    if (disposed || running || !visible()) return;
    running = true;
    try { await poll(); } catch { /* The caller owns error presentation. */ }
    finally {
      running = false;
      if (!disposed && visible()) timer = setTimeout(() => void run(), interval);
    }
  };
  const visibility = () => { clearTimeout(timer); void run(); };
  document.addEventListener("visibilitychange", visibility);
  timer = setTimeout(() => void run(), 0);
  return () => { disposed = true; clearTimeout(timer); document.removeEventListener("visibilitychange", visibility); };
}
