/** Fisher-Yates permutation of `0..length-1`. */
export function shuffledIndices(length: number): number[] {
  const idx = Array.from({ length }, (_, i) => i);
  for (let i = idx.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [idx[i], idx[j]] = [idx[j], idx[i]];
  }
  return idx;
}

/** `0..length-1` in order - the play order when shuffle is off. */
export function identityIndices(length: number): number[] {
  return Array.from({ length }, (_, i) => i);
}

/** Rotates `order` so `startValue` comes first, wrapping the rest around after
 * it. What the queue view renders: the playing track at the top, everything
 * still to come below it. */
export function rotateToStart(order: number[], startValue: number): number[] {
  const p = order.indexOf(startValue);
  if (p <= 0) return order;
  return [...order.slice(p), ...order.slice(0, p)];
}

/** How far `rotateToStart` shifted the order, i.e. the play-order position that
 * the rotated view shows first. Needed to turn a position the user dragged in
 * the view back into a position in the real order. */
export function rotationOffset(order: number[], startValue: number): number {
  return Math.max(0, order.indexOf(startValue));
}

/** True when `order` is a genuine permutation of `0..length-1`, which is what
 * the queue's shuffle order has to be for `indexOf`-based navigation to reach
 * every track exactly once. Worth checking on anything that came off disk. */
export function isPermutation(order: number[], length: number): boolean {
  if (order.length !== length) return false;
  if (new Set(order).size !== length) return false;
  return order.every((i) => Number.isInteger(i) && i >= 0 && i < length);
}
