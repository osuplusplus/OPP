// Opt in before launch with localStorage["opp:performance"] = "1".
// Payloads, arguments, resource IDs and credentials are never retained.
type Sample = { kind: string; name: string; duration_ms: number; json_bytes?: number };
const limit = 2048;
let enabled = false;
try { enabled = localStorage.getItem("opp:performance") === "1"; } catch { /* unavailable storage */ }
const samples: Sample[] = [];
function record(sample: Sample) {
  if (samples.length === limit) samples.shift();
  samples.push(sample);
}
export function measureCommand(name: string) {
  if (!enabled) return () => {};
  const started = performance.now();
  return (result?: unknown) => {
    const duration_ms = performance.now() - started;
    let json_bytes: number | undefined;
    try { json_bytes = new TextEncoder().encode(JSON.stringify(result) ?? "").length; } catch { /* non JSON response */ }
    record({ kind: "ipc", name, duration_ms, json_bytes });
  };
}
export function markInteractive() {
  if (enabled && !samples.some((sample) => sample.kind === "startup")) record({ kind: "startup", name: "frontend_interactive", duration_ms: performance.now() });
}
if (enabled) {
  if (typeof PerformanceObserver !== "undefined" && PerformanceObserver.supportedEntryTypes.includes("longtask")) {
    const observer = new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) record({ kind: "longtask", name: "main_thread", duration_ms: entry.duration });
    });
    observer.observe({ type: "longtask", buffered: true });
  }
  Object.assign(window, { oppPerformance: { snapshot: () => ({ captured_at: new Date().toISOString(), samples: samples.slice() }), reset: () => { samples.length = 0; } } });
}
