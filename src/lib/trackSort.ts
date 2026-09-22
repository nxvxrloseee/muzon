import type { Track } from "../types";

export type LibrarySort = "default" | "recentlyAdded" | "mostPlayed" | "recentlyPlayed";

export const LIBRARY_SORT_LABELS: Record<LibrarySort, string> = {
  default: "По исполнителю",
  recentlyAdded: "Недавно добавленные",
  mostPlayed: "Часто слушаю",
  recentlyPlayed: "Недавно слушал",
};

/** Ties break on title so the order is stable rather than however the previous
 * sort happened to leave things - two tracks added in the same second, or both
 * never played, shouldn't swap places on re-render. */
function byTitle(a: Track, b: Track): number {
  return a.title.localeCompare(b.title);
}

/**
 * These are sorts, not filters: a never-played track still appears under
 * "часто слушаю", just at the bottom. Hiding it would make the library look
 * like it had lost tracks.
 */
export function sortTracks(tracks: Track[], sort: LibrarySort): Track[] {
  // The library already arrives ordered by artist/album/track number, so the
  // default costs nothing - and returning the very same array keeps downstream
  // memos from invalidating.
  if (sort === "default") return tracks;

  const sorted = [...tracks];
  switch (sort) {
    case "recentlyAdded":
      sorted.sort((a, b) => b.added_at - a.added_at || byTitle(a, b));
      break;
    case "mostPlayed":
      sorted.sort((a, b) => b.play_count - a.play_count || byTitle(a, b));
      break;
    case "recentlyPlayed":
      sorted.sort(
        (a, b) => (b.last_played_at ?? 0) - (a.last_played_at ?? 0) || byTitle(a, b),
      );
      break;
  }
  return sorted;
}
