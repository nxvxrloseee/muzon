import { identityIndices, rotateToStart } from "./shuffle";

/**
 * The queue's three interdependent pieces: the tracks, a permutation of indices
 * into them that is the shuffled play order, and the index of what's playing.
 *
 * Every mutation has to keep the permutation whole and keep `cursor` pointing
 * at the same *track* rather than at the same number - which is exactly the
 * kind of index arithmetic that belongs outside a store and under test.
 */
export interface QueueShape<T> {
  queue: T[];
  shuffleOrder: number[];
  cursor: number;
}

/** Inserts at `queueIndex` in the queue and at `orderPosition` in the shuffle
 * order, shifting every index that the insertion pushed along. */
function insert<T>(
  shape: QueueShape<T>,
  tracks: T[],
  queueIndex: number,
  orderPosition: number,
): QueueShape<T> {
  if (tracks.length === 0) return shape;

  const shift = (i: number) => (i >= queueIndex ? i + tracks.length : i);
  const inserted = tracks.map((_, k) => queueIndex + k);
  // `map` keeps array positions, so `orderPosition` still means the same slot.
  const shifted = shape.shuffleOrder.map(shift);

  return {
    queue: [...shape.queue.slice(0, queueIndex), ...tracks, ...shape.queue.slice(queueIndex)],
    shuffleOrder: [
      ...shifted.slice(0, orderPosition),
      ...inserted,
      ...shifted.slice(orderPosition),
    ],
    cursor: shape.cursor < 0 ? shape.cursor : shift(shape.cursor),
  };
}

/** Appends to the end of the queue and to the end of the play order. */
export function appendTracks<T>(shape: QueueShape<T>, tracks: T[]): QueueShape<T> {
  return insert(shape, tracks, shape.queue.length, shape.shuffleOrder.length);
}

/**
 * Inserts so the tracks play immediately after the current one. Patches both
 * the queue position and the shuffle position rather than picking one, so "play
 * next" means the same thing whether shuffle is on or off.
 */
export function insertAfterCursor<T>(shape: QueueShape<T>, tracks: T[]): QueueShape<T> {
  if (shape.cursor < 0) return appendTracks(shape, tracks);
  return insert(shape, tracks, shape.cursor + 1, shape.shuffleOrder.indexOf(shape.cursor) + 1);
}

export interface RemovalResult<T> {
  shape: QueueShape<T>;
  /** True when the removed track was the one playing, so the caller has to do
   * something about the audio rather than only the list. */
  wasCurrent: boolean;
  /** Where playback should continue, as an index into the *new* queue: whatever
   * followed the removed track in the active order, or -1 when it was the last
   * one. Only meaningful together with `wasCurrent`. */
  nextCursor: number;
}

export function removeAt<T>(
  shape: QueueShape<T>,
  queueIndex: number,
  shuffle: boolean,
): RemovalResult<T> {
  if (queueIndex < 0 || queueIndex >= shape.queue.length) {
    return { shape, wasCurrent: false, nextCursor: -1 };
  }

  const wasCurrent = shape.cursor === queueIndex;
  const order = shuffle ? shape.shuffleOrder : identityIndices(shape.queue.length);
  // Read the follower before the removal, while the old indices still hold.
  const followingBefore = wasCurrent ? (order[order.indexOf(queueIndex) + 1] ?? -1) : -1;

  const shift = (i: number) => (i > queueIndex ? i - 1 : i);
  const nextCursor = followingBefore < 0 ? -1 : shift(followingBefore);

  return {
    shape: {
      queue: shape.queue.filter((_, i) => i !== queueIndex),
      shuffleOrder: shape.shuffleOrder.filter((i) => i !== queueIndex).map(shift),
      cursor: wasCurrent ? nextCursor : shift(shape.cursor),
    },
    wasCurrent,
    nextCursor,
  };
}

/**
 * Applies a drag made in the queue view, addressed by position *in that view* -
 * which shows the play order rotated so the playing track comes first.
 *
 * That rotation is why this cannot simply move `order[from]` to `order[to]`: a
 * drag can cross the point where the view wraps back round to the start of the
 * order, and the two positions then no longer mean the same thing. So the move
 * is applied to the sequence as displayed and that sequence becomes the new
 * order outright - which it may, because the playing track heads it either way.
 */
export function moveInView<T>(
  shape: QueueShape<T>,
  shuffle: boolean,
  fromView: number,
  toView: number,
): QueueShape<T> {
  const length = shape.queue.length;
  const inRange = (p: number) => p >= 0 && p < length;
  if (fromView === toView || !inRange(fromView) || !inRange(toView)) return shape;

  const order = shuffle ? shape.shuffleOrder : identityIndices(length);
  const moved = [...rotateToStart(order, shape.cursor)];
  const [entry] = moved.splice(fromView, 1);
  moved.splice(toView, 0, entry);

  if (shuffle) {
    return { ...shape, shuffleOrder: moved };
  }

  // With shuffle off the play order *is* the queue, so the queue itself has to
  // be rewritten - and every index into it re-pointed at the track it named.
  const newIndexOf = new Map(moved.map((oldIndex, newIndex) => [oldIndex, newIndex]));
  const remap = (i: number) => newIndexOf.get(i) ?? i;
  return {
    queue: moved.map((oldIndex) => shape.queue[oldIndex]),
    shuffleOrder: shape.shuffleOrder.map(remap),
    cursor: shape.cursor < 0 ? shape.cursor : remap(shape.cursor),
  };
}
