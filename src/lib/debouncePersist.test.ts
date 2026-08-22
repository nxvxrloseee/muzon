import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createDebouncedPersist, flushAllPendingPersists } from "./debouncePersist";

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("createDebouncedPersist", () => {
  it("only runs the debounced action once the delay elapses", () => {
    const persist = createDebouncedPersist(300);
    const run = vi.fn();
    persist.schedule(run);

    vi.advanceTimersByTime(299);
    expect(run).not.toHaveBeenCalled();

    vi.advanceTimersByTime(1);
    expect(run).toHaveBeenCalledTimes(1);
  });

  it("a later schedule() replaces the pending run instead of running both", () => {
    const persist = createDebouncedPersist(300);
    const first = vi.fn();
    const second = vi.fn();
    persist.schedule(first);
    vi.advanceTimersByTime(100);
    persist.schedule(second);
    vi.advanceTimersByTime(300);

    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledTimes(1);
  });

  it("flush() runs a pending action immediately and cancels the timer", async () => {
    const persist = createDebouncedPersist(300);
    const run = vi.fn();
    persist.schedule(run);

    await persist.flush();
    expect(run).toHaveBeenCalledTimes(1);

    // The timer must not fire a second time after being flushed.
    vi.advanceTimersByTime(300);
    expect(run).toHaveBeenCalledTimes(1);
  });

  it("flush() with nothing pending is a no-op", async () => {
    const persist = createDebouncedPersist(300);
    await expect(persist.flush()).resolves.toBeUndefined();
  });

  it("never rejects even if the scheduled action rejects and didn't self-catch", async () => {
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const persist = createDebouncedPersist(300);
    persist.schedule(() => Promise.reject(new Error("boom")));

    await expect(persist.flush()).resolves.toBeUndefined();
    expect(errorSpy).toHaveBeenCalledWith("Debounced persist failed", expect.any(Error));

    errorSpy.mockRestore();
  });

  it("never rejects even if the scheduled action throws synchronously", async () => {
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const persist = createDebouncedPersist(300);
    persist.schedule(() => {
      throw new Error("sync boom");
    });

    await expect(persist.flush()).resolves.toBeUndefined();
    expect(errorSpy).toHaveBeenCalledWith("Debounced persist failed", expect.any(Error));

    errorSpy.mockRestore();
  });
});

describe("flushAllPendingPersists", () => {
  it("flushes every debounced persist created so far, independently", async () => {
    const a = createDebouncedPersist(300);
    const b = createDebouncedPersist(300);
    const runA = vi.fn();
    const runB = vi.fn();
    a.schedule(runA);
    b.schedule(runB);

    await flushAllPendingPersists();

    expect(runA).toHaveBeenCalledTimes(1);
    expect(runB).toHaveBeenCalledTimes(1);
  });

  it("still resolves and flushes the rest even if one instance's action fails", async () => {
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const a = createDebouncedPersist(300);
    const b = createDebouncedPersist(300);
    const runB = vi.fn();
    a.schedule(() => Promise.reject(new Error("boom")));
    b.schedule(runB);

    await expect(flushAllPendingPersists()).resolves.toBeUndefined();
    expect(runB).toHaveBeenCalledTimes(1);

    errorSpy.mockRestore();
  });
});
