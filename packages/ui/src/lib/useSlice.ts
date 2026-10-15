import { useEffect, useRef, useState } from 'react';

export interface Slice<T> {
  total: number;
  start: number;
  rows: T[];
}

/**
 * Requests only the visible window of a virtualised list from the engine, and re-requests when the engine
 * sequence changes. Requests are coalesced to one per animation frame and at most 30 per second (SPEC 5).
 */
export function useSlice<T>(
  fetch: (start: number, count: number) => Promise<Slice<T> & Record<string, unknown>>,
  key: string,
  start: number,
  end: number,
  seq: number | string,
): Slice<T> & { extra: Record<string, unknown> } {
  const [slice, setSlice] = useState<Slice<T> & { extra: Record<string, unknown> }>({ total: 0, start: 0, rows: [], extra: {} });
  const fetchRef = useRef(fetch);
  fetchRef.current = fetch;
  const want = useRef({ start, end, key });
  want.current = { start, end, key };
  const pending = useRef(false);
  const last = useRef(0);
  const inflight = useRef(false);
  const again = useRef(false);
  const lastKey = useRef(key);

  useEffect(() => {
    const doFetch = async () => {
      pending.current = false;
      if (inflight.current) {
        again.current = true;
        return;
      }
      inflight.current = true;
      last.current = Date.now();
      const { start: s, end: e, key: k } = want.current;
      const from = Math.max(0, s);
      const count = Math.max(1, e - from + 1);
      try {
        const res = await fetchRef.current(from, count);
        if (k === want.current.key) {
          const { total, start: rs, rows, ...extra } = res;
          setSlice({ total, start: rs, rows, extra });
        }
      } catch {
        /* connection closed: keep the last slice */
      } finally {
        inflight.current = false;
        if (again.current) {
          again.current = false;
          schedule();
        }
      }
    };
    const schedule = () => {
      if (pending.current) return;
      pending.current = true;
      const wait = Math.max(0, 33 - (Date.now() - last.current));
      const go = () => (typeof requestAnimationFrame === 'function' ? requestAnimationFrame(() => void doFetch()) : void doFetch());
      if (wait > 0) setTimeout(go, wait);
      else go();
    };
    if (lastKey.current !== key) {
      lastKey.current = key;
      pending.current = false;
    }
    schedule();
  }, [key, start, end, seq]);

  return slice;
}
