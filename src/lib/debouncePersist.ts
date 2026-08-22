/** A debounced "persist" action that can also be flushed on demand - so a
 * pending disk/DB write isn't silently dropped when the app closes, or (for
 * per-track settings like tempo) when the thing it's about is switched away
 * from before the debounce window elapses. Every instance auto-registers so
 * `flushAllPendingPersists` can catch all of them in one place, instead of
 * every call site needing to know which stores currently debounce a save. */
export interface DebouncedPersist {
  /** Replaces any pending save with `run` and restarts the delay. */
  schedule: (run: () => Promise<unknown> | void) => void;
  /** Cancels the pending timer and runs `run` immediately, if one is pending.
   * Always resolves, never rejects - a throwing/rejecting `run` is caught and
   * logged here, not left to whoever called `schedule()`. This is what makes
   * `flushAllPendingPersists()` safe to await unconditionally on window close:
   * it can't hang or reject just because some future debounced setting forgot
   * its own `.catch()`. */
  flush: () => Promise<void>;
}

const registry: DebouncedPersist[] = [];

export function createDebouncedPersist(delayMs: number): DebouncedPersist {
  let timer: ReturnType<typeof setTimeout> | null = null;
  let pending: (() => Promise<unknown> | void) | null = null;

  function flush(): Promise<void> {
    if (timer) clearTimeout(timer);
    timer = null;
    if (!pending) return Promise.resolve();
    const run = pending;
    pending = null;
    try {
      return Promise.resolve(run())
        .then(() => undefined)
        .catch((e) => console.error("Debounced persist failed", e));
    } catch (e) {
      console.error("Debounced persist failed", e);
      return Promise.resolve();
    }
  }

  function schedule(run: () => Promise<unknown> | void) {
    if (timer) clearTimeout(timer);
    pending = run;
    timer = setTimeout(() => {
      timer = null;
      void flush();
    }, delayMs);
  }

  const instance: DebouncedPersist = { schedule, flush };
  registry.push(instance);
  return instance;
}

/** Flushes every debounced persist created via `createDebouncedPersist` across
 * the whole app (EQ, theme, crossfade, per-track tempo, ...) - call this
 * before the window is actually allowed to close. */
export function flushAllPendingPersists(): Promise<void> {
  return Promise.all(registry.map((p) => p.flush())).then(() => undefined);
}
