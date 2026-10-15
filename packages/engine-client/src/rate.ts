// The rate shown on the connection tabs and the tree rows (change set 9): a one-second average refreshed once a
// second. The figure is the number of messages received in the last completed one-second window, so it changes
// only when a new second starts and the first value appears after the first full second. The Rust engine
// (crates/engine/src/store.rs, RateWindow::rate) applies the same rule; the mock engine uses this helper.

/** Start (ms) of the last completed one-second window before `now`. */
export function lastSecondStart(now: number): number {
  return Math.floor(now / 1000) * 1000 - 1000;
}

/**
 * Messages per second for receive times `times` (ascending, ms): the count of those in the last completed second
 * before `now`. Times in the running second do not count yet.
 */
export function lastSecondRate(times: readonly number[], now: number): number {
  const from = lastSecondStart(now);
  const to = from + 1000;
  let n = 0;
  for (let i = times.length - 1; i >= 0; i--) {
    const t = times[i];
    if (t >= to) continue;
    if (t < from) break;
    n++;
  }
  return n;
}

/** Drops the receive times that can no longer count (older than the last completed second). */
export function pruneTimes(times: number[], now: number): void {
  const from = lastSecondStart(now);
  let i = 0;
  while (i < times.length && times[i] < from) i++;
  if (i > 0) times.splice(0, i);
}
