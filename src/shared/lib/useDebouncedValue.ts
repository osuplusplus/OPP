import { useEffect, useState } from "react";

/** Delay expensive IPC searches until input has settled; rendering stays immediate. */
export function useDebouncedValue<T>(value: T, delay = 200): T {
  const [settled, setSettled] = useState(value);
  useEffect(() => {
    const timer = window.setTimeout(() => setSettled(() => value), delay);
    return () => window.clearTimeout(timer);
  }, [value, delay]);
  return settled;
}
