import { useCallback, useEffect, useRef, useState } from "react";

export type SaveStatus = "idle" | "saving" | "saved" | "error";

/**
 * Debounced save. `schedule` queues the latest value; `flush` saves it now.
 * Pending changes are flushed on unmount so nothing typed is lost.
 */
export function useAutosave<T>(save: (value: T) => Promise<void>, delayMs = 500) {
  const [status, setStatus] = useState<SaveStatus>("idle");
  const pending = useRef<{ value: T } | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const saveRef = useRef(save);
  saveRef.current = save;

  const flush = useCallback(async () => {
    clearTimeout(timer.current);
    const next = pending.current;
    if (!next) return;
    pending.current = null;
    setStatus("saving");
    try {
      await saveRef.current(next.value);
      setStatus(pending.current ? "saving" : "saved");
    } catch {
      setStatus("error");
    }
  }, []);

  const schedule = useCallback(
    (value: T) => {
      pending.current = { value };
      setStatus("saving");
      clearTimeout(timer.current);
      timer.current = setTimeout(flush, delayMs);
    },
    [flush, delayMs],
  );

  /** Drops unsaved changes, e.g. when an import replaces the whole value. */
  const cancel = useCallback(() => {
    clearTimeout(timer.current);
    pending.current = null;
    setStatus("idle");
  }, []);

  useEffect(() => () => void flush(), [flush]);

  return { status, schedule, flush, cancel };
}
