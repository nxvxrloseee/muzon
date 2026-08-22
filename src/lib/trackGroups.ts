import type { Track } from "../types";

export const UNKNOWN_ARTIST = "Неизвестный исполнитель";
export const UNKNOWN_ALBUM = "Неизвестный альбом";

export interface AlbumGroup {
  key: string;
  artist: string;
  album: string;
  tracks: Track[];
}

export function groupAlbums(tracks: Track[]): AlbumGroup[] {
  const map = new Map<string, AlbumGroup>();
  for (const t of tracks) {
    const artist = t.artist ?? UNKNOWN_ARTIST;
    const album = t.album ?? UNKNOWN_ALBUM;
    const key = `${artist} ${album}`;
    let group = map.get(key);
    if (!group) {
      group = { key, artist, album, tracks: [] };
      map.set(key, group);
    }
    group.tracks.push(t);
  }
  for (const group of map.values()) {
    group.tracks.sort((a, b) => (a.track_no ?? 0) - (b.track_no ?? 0));
  }
  return Array.from(map.values()).sort(
    (a, b) => a.artist.localeCompare(b.artist) || a.album.localeCompare(b.album),
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
          (a.album ?? "").localeCompare(b.album ?? "") || (a.track_no ?? 0) - (b.track_no ?? 0),
      ),
    }))
    .sort((a, b) => a.artist.localeCompare(b.artist));
}
