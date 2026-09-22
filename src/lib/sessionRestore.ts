import { isPermutation, shuffledIndices } from "./shuffle";
import type { Session, Track } from "../types";

export interface RestoredQueue {
  queue: Track[];
  shuffleOrder: number[];
  cursor: number;
}

/** A session with every field present. */
export type RestorableSession = Required<Session>;

/**
 * The stored session is `#[serde(default)]` on the Rust side, so a file written
 * by an older build simply omits whatever it didn't have yet and every field
 * arrives optional. This fills the gaps once, at the boundary, instead of
 * leaving `?? 0` scattered over everything downstream.
 */
export function normalizeSession(session: Session): RestorableSession {
  return {
    queuePaths: session.queuePaths ?? [],
    shuffleOrder: session.shuffleOrder ?? [],
    cursor: session.cursor ?? -1,
    shuffle: session.shuffle ?? false,
    repeat: session.repeat ?? "off",
    positionSecs: session.positionSecs ?? 0,
    volume: session.volume ?? 1,
  };
}

/**
 * Turns a saved session's list of paths back into a queue of live library
 * tracks, or null when nothing of it survives.
 *
 * Not a straight lookup, because files come and go between runs: anything no
 * longer in the library is dropped, and the saved cursor and shuffle
 * permutation are re-indexed onto whatever is left. Without that re-indexing a
 * single deleted track would shift the whole queue by one and resume on the
 * wrong song. A shuffle order that doesn't survive as a valid permutation is
 * redrawn rather than left to send `playNext` somewhere unreachable.
 */
export function rebuildQueue(session: RestorableSession, tracks: Track[]): RestoredQueue | null {
  const byPath = new Map(tracks.map((track) => [track.path, track]));
  const queue: Track[] = [];
  const newIndexOf = new Map<number, number>();

  session.queuePaths.forEach((path, oldIndex) => {
    const track = byPath.get(path);
    if (!track) return;
    newIndexOf.set(oldIndex, queue.length);
    queue.push(track);
  });

  if (queue.length === 0) return null;

  const shuffleOrder = session.shuffleOrder
    .map((oldIndex) => newIndexOf.get(oldIndex))
    .filter((index): index is number => index !== undefined);

  return {
    queue,
    shuffleOrder: isPermutation(shuffleOrder, queue.length)
      ? shuffleOrder
      : shuffledIndices(queue.length),
    cursor: newIndexOf.get(session.cursor) ?? -1,
  };
}
