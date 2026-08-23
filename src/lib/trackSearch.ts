import type { Track } from "../types";

/**
 * Lowercased "title / artist / album" blob per track, built once and cached
 * against the track object itself. Filtering used to lowercase all three fields
 * of every track on every keystroke, which for a large library is a few
 * hundred thousand throwaway strings per typed character.
 *
 * A WeakMap needs no invalidation: editing a track's tags replaces the object
 * in the library store, so the stale entry becomes unreachable on its own.
 */
const haystacks = new WeakMap<Track, string>();

function haystack(track: Track): string {
  let value = haystacks.get(track);
  if (value === undefined) {
    value = `${track.title}\n${track.artist ?? ""}\n${track.album ?? ""}`.toLowerCase();
    haystacks.set(track, value);
  }
  return value;
}

export function trackMatchesQuery(track: Track, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return haystack(track).includes(q);
}

/** Returns the input array unchanged for an empty query, so an untouched search
 * box costs nothing and doesn't invalidate downstream memos. */
export function filterTracks(tracks: Track[], query: string): Track[] {
  const q = query.trim().toLowerCase();
  if (!q) return tracks;
  return tracks.filter((track) => haystack(track).includes(q));
}
