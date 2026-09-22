import { describe, expect, it } from "vitest";
import { identityIndices, isPermutation, rotateToStart } from "./shuffle";
import {
  appendTracks,
  insertAfterCursor,
  moveInView,
  removeAt,
  type QueueShape,
} from "./queueOps";

/** What the queue drawer actually shows: the play order, rotated so the
 * playing track is first. Assertions read better against this than against the
 * index arrays behind it. */
function viewOf(shape: QueueShape<string>, shuffle: boolean): string[] {
  const order = shuffle ? shape.shuffleOrder : identityIndices(shape.queue.length);
  return rotateToStart(order, shape.cursor).map((i) => shape.queue[i]);
}

/** The invariant every operation has to preserve: the shuffle order stays a
 * permutation of the queue, so navigation reaches every track exactly once. */
function expectConsistent(shape: QueueShape<string>) {
  expect(isPermutation(shape.shuffleOrder, shape.queue.length)).toBe(true);
  expect(shape.cursor).toBeGreaterThanOrEqual(-1);
  expect(shape.cursor).toBeLessThan(shape.queue.length);
}

function shape(
  queue: string[],
  shuffleOrder: number[],
  cursor: number,
): QueueShape<string> {
  return { queue, shuffleOrder, cursor };
}

const abc = () => shape(["a", "b", "c"], [2, 0, 1], 0);

describe("appendTracks", () => {
  it("adds to the end of both the queue and the play order", () => {
    const next = appendTracks(abc(), ["d"]);
    expect(next.queue).toEqual(["a", "b", "c", "d"]);
    expect(next.shuffleOrder).toEqual([2, 0, 1, 3]);
    expect(next.cursor).toBe(0);
    expectConsistent(next);
  });

  it("is a no-op for an empty addition", () => {
    const before = abc();
    expect(appendTracks(before, [])).toBe(before);
  });

  it("works on an empty queue", () => {
    const next = appendTracks(shape([], [], -1), ["a", "b"]);
    expect(next.queue).toEqual(["a", "b"]);
    expect(next.shuffleOrder).toEqual([0, 1]);
    expect(next.cursor).toBe(-1);
    expectConsistent(next);
  });
});

describe("insertAfterCursor", () => {
  it("puts the track next in queue order and next in shuffle order at once", () => {
    // Playing "a" (index 0); shuffle order is c, a, b.
    const next = insertAfterCursor(abc(), ["x"]);
    expect(next.queue).toEqual(["a", "x", "b", "c"]);
    // "a" is still what's playing...
    expect(next.queue[next.cursor]).toBe("a");
    // ...and "x" follows it in both readings of "next".
    expect(next.queue[next.cursor + 1]).toBe("x");
    const orderPos = next.shuffleOrder.indexOf(next.cursor);
    expect(next.queue[next.shuffleOrder[orderPos + 1]]).toBe("x");
    expectConsistent(next);
  });

  it("keeps the cursor on the same track when the insert lands before it", () => {
    // Playing "c" (index 2): inserting after it shifts nothing below it.
    const next = insertAfterCursor(shape(["a", "b", "c"], [0, 1, 2], 2), ["x"]);
    expect(next.queue[next.cursor]).toBe("c");
    expectConsistent(next);
  });

  it("falls back to appending when nothing is playing", () => {
    const next = insertAfterCursor(shape(["a"], [0], -1), ["x"]);
    expect(next.queue).toEqual(["a", "x"]);
    expect(next.cursor).toBe(-1);
    expectConsistent(next);
  });
});

describe("removeAt", () => {
  it("shifts the cursor down when something before it goes", () => {
    const { shape: next, wasCurrent } = removeAt(shape(["a", "b", "c"], [0, 1, 2], 2), 0, false);
    expect(wasCurrent).toBe(false);
    expect(next.queue).toEqual(["b", "c"]);
    expect(next.queue[next.cursor]).toBe("c");
    expectConsistent(next);
  });

  it("leaves the cursor alone when something after it goes", () => {
    const { shape: next } = removeAt(shape(["a", "b", "c"], [0, 1, 2], 0), 2, false);
    expect(next.queue[next.cursor]).toBe("a");
    expectConsistent(next);
  });

  it("reports where to continue when the playing track is removed", () => {
    const { wasCurrent, nextCursor, shape: next } = removeAt(
      shape(["a", "b", "c"], [0, 1, 2], 0),
      0,
      false,
    );
    expect(wasCurrent).toBe(true);
    expect(next.queue[nextCursor]).toBe("b");
    expect(next.cursor).toBe(nextCursor);
    expectConsistent(next);
  });

  it("follows the shuffle order, not the queue order, when shuffled", () => {
    // Shuffle order c, a, b - playing "c", so "a" is what comes next.
    const { nextCursor, shape: next } = removeAt(shape(["a", "b", "c"], [2, 0, 1], 2), 2, true);
    expect(next.queue[nextCursor]).toBe("a");
    expectConsistent(next);
  });

  it("reports no continuation when the last track in the order is removed", () => {
    const { wasCurrent, nextCursor, shape: next } = removeAt(
      shape(["a", "b"], [0, 1], 1),
      1,
      false,
    );
    expect(wasCurrent).toBe(true);
    expect(nextCursor).toBe(-1);
    expect(next.cursor).toBe(-1);
    expectConsistent(next);
  });

  it("empties cleanly", () => {
    const { shape: next, nextCursor } = removeAt(shape(["a"], [0], 0), 0, false);
    expect(next.queue).toEqual([]);
    expect(nextCursor).toBe(-1);
    expectConsistent(next);
  });

  it("ignores an out-of-range index", () => {
    const before = abc();
    expect(removeAt(before, 9, false).shape).toBe(before);
    expect(removeAt(before, -1, false).shape).toBe(before);
  });
});

describe("moveInView", () => {
  it("reorders what the view shows, with shuffle off", () => {
    // Playing "a", so the view already reads a, b, c.
    const before = shape(["a", "b", "c"], [0, 1, 2], 0);
    const next = moveInView(before, false, 2, 1);
    expect(viewOf(next, false)).toEqual(["a", "c", "b"]);
    expect(next.queue[next.cursor]).toBe("a");
    expectConsistent(next);
  });

  it("addresses positions in the rotated view, not in the queue", () => {
    // Playing "c" (queue index 2), so the view reads c, a, b - dragging view
    // position 2 ("b") to position 1 must put "b" ahead of "a", even though in
    // queue terms "b" sits before both.
    const next = moveInView(shape(["a", "b", "c"], [0, 1, 2], 2), false, 2, 1);
    expect(viewOf(next, false)).toEqual(["c", "b", "a"]);
    expect(next.queue[next.cursor]).toBe("c");
    expectConsistent(next);
  });

  it("survives a drag across the point where the view wraps", () => {
    // Playing "d": the view is d, e, a, b, c, so view position 4 ("c") sits
    // before the wrap in the underlying order and position 1 ("e") after it.
    const before = shape(["a", "b", "c", "d", "e"], [0, 1, 2, 3, 4], 3);
    expect(viewOf(before, false)).toEqual(["d", "e", "a", "b", "c"]);
    const next = moveInView(before, false, 4, 1);
    expect(viewOf(next, false)).toEqual(["d", "c", "e", "a", "b"]);
    expect(next.queue[next.cursor]).toBe("d");
    expectConsistent(next);
  });

  it("leaves the queue alone and moves only the permutation when shuffled", () => {
    // Shuffle order c, a, b and playing "c", so the view reads c, a, b.
    const before = shape(["a", "b", "c"], [2, 0, 1], 2);
    const next = moveInView(before, true, 2, 1);
    expect(next.queue).toEqual(["a", "b", "c"]);
    expect(viewOf(next, true)).toEqual(["c", "b", "a"]);
    expectConsistent(next);
  });

  it("keeps the queue playable when nothing is playing yet", () => {
    const next = moveInView(shape(["a", "b", "c"], [0, 1, 2], -1), false, 0, 2);
    expect(next.queue).toEqual(["b", "c", "a"]);
    expect(next.cursor).toBe(-1);
    expectConsistent(next);
  });

  it("is a no-op for a move that goes nowhere or out of range", () => {
    const before = abc();
    expect(moveInView(before, false, 1, 1)).toBe(before);
    expect(moveInView(before, false, 0, 9)).toBe(before);
    expect(moveInView(before, false, -1, 0)).toBe(before);
  });
});
