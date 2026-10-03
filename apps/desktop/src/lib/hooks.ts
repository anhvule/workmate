import { useCallback, useEffect, useRef, useState } from "react";

export interface Loaded<T> {
  data: T | undefined;
  error: string | null;
  loading: boolean;
  reload: () => void;
}

export const messageOf = (e: unknown): string => (e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e));

/**
 * Load something from Rust and reload when `deps` change or `reload()` is called.
 *
 * Results that arrive after the inputs changed are dropped, so a slow answer to
 * an old question can never overwrite a fast answer to the current one. The data
 * is read-through, never a cache we write to: the webview persists nothing.
 */
export function useLoad<T>(fn: () => Promise<T>, deps: readonly unknown[]): Loaded<T> {
  const [data, setData] = useState<T | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [tick, setTick] = useState(0);
  const fnRef = useRef(fn);
  fnRef.current = fn;

  useEffect(() => {
    let live = true;
    setLoading(true);
    fnRef.current().then(
      (v) => {
        if (!live) return;
        setData(v);
        setError(null);
        setLoading(false);
      },
      (e: unknown) => {
        if (!live) return;
        setError(messageOf(e));
        setLoading(false);
      },
    );
    return () => {
      live = false;
    };
  // The caller owns `deps`; `fn` is read through a ref so it need not be stable.
  }, [tick, ...deps]);

  const reload = useCallback(() => setTick((t) => t + 1), []);
  return { data, error, loading, reload };
}

/** Run an action with pending and error state, for buttons. */
export function useAction<A extends unknown[]>(fn: (...a: A) => Promise<unknown>) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const run = useCallback(
    async (...a: A): Promise<boolean> => {
      setPending(true);
      setError(null);
      try {
        await fn(...a);
        return true;
      } catch (e) {
        setError(messageOf(e));
        return false;
      } finally {
        setPending(false);
      }
    },
    [fn],
  );
  return { run, pending, error, clear: () => setError(null) };
}
