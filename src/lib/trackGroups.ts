import type { Track } from "../types";

export const UNKNOWN_ARTIST = "Неизвестный исполнитель";
export const UNKNOWN_ALBUM = "Неизвестный альбом";
export const UNKNOWN_GENRE = "Без жанра";

/** `localeCompare` with no arguments builds a collator on every call; sorting
 * a few thousand groups that way is most of what regrouping costs. Same
 * default locale, so the order is unchanged. */
const collator = new Intl.Collator();
const compare = collator.compare;

export interface AlbumGroup {
  key: string;
  artist: string;
  album: string;
  /** The earliest year any of its tracks is tagged with, if any is. */
  year: number | null;
  tracks: Track[];
}

/** Unknown years sort after known ones, so a discography reads oldest first. */
function compareYears(a: number | null, b: number | null): number {
  return (a ?? Infinity) - (b ?? Infinity) || 0;
}

export function groupAlbums(tracks: Track[]): AlbumGroup[] {
  const map = new Map<string, AlbumGroup>();
  for (const t of tracks) {
    const artist = t.artist ?? UNKNOWN_ARTIST;
    const album = t.album ?? UNKNOWN_ALBUM;
    const key = `${artist} ${album}`;
    let group = map.get(key);
    if (!group) {
      group = { key, artist, album, year: null, tracks: [] };
      map.set(key, group);
    }
    group.tracks.push(t);
    if (t.year != null && (group.year == null || t.year < group.year)) group.year = t.year;
  }
  for (const group of map.values()) {
    group.tracks.sort((a, b) => (a.track_no ?? 0) - (b.track_no ?? 0));
  }
  return Array.from(map.values()).sort(
    (a, b) =>
      compare(a.artist, b.artist) || compareYears(a.year, b.year) || compare(a.album, b.album),
  );
}

export interface ArtistGroup {
  artist: string;
  tracks: Track[];
}

export function groupArtists(tracks: Track[]): ArtistGroup[] {
  const map = new Map<string, Track[]>();
  for (const t of tracks) {
    const artist = t.artist ?? UNKNOWN_ARTIST;
    if (!map.has(artist)) map.set(artist, []);
    map.get(artist)!.push(t);
  }
  return Array.from(map.entries())
    .map(([artist, groupTracks]) => ({
      artist,
      tracks: [...groupTracks].sort(
        (a, b) =>
          compareYears(a.year, b.year) ||
          compare(a.album ?? "", b.album ?? "") ||
          (a.track_no ?? 0) - (b.track_no ?? 0),
      ),
    }))
    .sort((a, b) => compare(a.artist, b.artist));
}

export interface GenreGroup {
  genre: string;
  tracks: Track[];
}

/** Tracks keep the library's own artist/album/track order within a genre. */
export function groupGenres(tracks: Track[]): GenreGroup[] {
  const map = new Map<string, Track[]>();
  for (const t of tracks) {
    const genre = t.genre ?? UNKNOWN_GENRE;
    let group = map.get(genre);
    if (!group) {
      group = [];
      map.set(genre, group);
    }
    group.push(t);
  }
  return Array.from(map, ([genre, groupTracks]) => ({ genre, tracks: groupTracks })).sort(
    (a, b) =>
      // The untagged pile goes last rather than wherever "Б" happens to sort
      Number(a.genre === UNKNOWN_GENRE) - Number(b.genre === UNKNOWN_GENRE) ||
      compare(a.genre, b.genre),
  );
}

/**
 * Swaps every group whose tracks are exactly the ones it had last time (same
 * objects, same order) back to its previous object, and returns `prev` itself
 * when nothing changed at all.
 *
 * Liking a track or counting a listen replaces one track object in the library,
 * which regroups everything from scratch - and fresh group objects defeat the
 * `memo` on every visible card, although only one group actually differs.
 */
export function keepUnchangedGroups<G extends { tracks: Track[] }>(
  prev: G[],
  next: G[],
  keyOf: (group: G) => string,
): G[] {
  const byKey = new Map(prev.map((g) => [keyOf(g), g]));
  let identical = prev.length === next.length;
  const result = next.map((group, i) => {
    const old = byKey.get(keyOf(group));
    const same =
      old !== undefined &&
      old.tracks.length === group.tracks.length &&
      old.tracks.every((t, j) => t === group.tracks[j]);
    if (!same || old !== prev[i]) identical = false;
    return same ? old : group;
  });
  return identical ? prev : result;
}
