import type { Track } from "../types";

export type SmartPlaylistId =
  | "favorites"
  | "most-played"
  | "recently-played"
  | "forgotten"
  | "never-played"
  | "recently-added";

export interface SmartPlaylist {
  id: SmartPlaylistId;
  name: string;
  /** Shown when nothing matches, saying what would make something match. */
  empty: string;
  /** `now` in unix seconds, passed in so the rules are testable. */
  select: (tracks: Track[], now: number) => Track[];
}

const DAY = 24 * 60 * 60;

/** Enough to fill an evening; a "top" list of the whole library isn't one. */
export const TOP_LIMIT = 100;

/** Played this often means it was liked, whether or not it got a heart. */
export const FORGOTTEN_MIN_PLAYS = 3;
export const FORGOTTEN_AFTER_DAYS = 60;
export const RECENT_DAYS = 30;

const byTitle = (a: Track, b: Track) => a.title.localeCompare(b.title);

export const SMART_PLAYLISTS: SmartPlaylist[] = [
  {
    id: "favorites",
    name: "Любимые",
    empty: "Пока нет любимых треков — отметьте их сердечком в библиотеке",
    // Library order, so an album liked whole plays in order
    select: (tracks) => tracks.filter((t) => t.is_favorite),
  },
  {
    id: "most-played",
    name: "Часто слушаю",
    empty: "Здесь появятся треки, которые вы слушаете чаще всего",
    select: (tracks) =>
      tracks
        .filter((t) => t.play_count > 0)
        .sort((a, b) => b.play_count - a.play_count || byTitle(a, b))
        .slice(0, TOP_LIMIT),
  },
  {
    id: "recently-played",
    name: "Недавно слушал",
    empty: "Здесь появятся недавно прослушанные треки",
    select: (tracks) =>
      tracks
        .filter((t) => t.last_played_at != null)
        .sort((a, b) => b.last_played_at! - a.last_played_at!)
        .slice(0, TOP_LIMIT),
  },
  {
    id: "forgotten",
    name: "Давно не слушал",
    empty: `Здесь появятся треки, которые вы слушали хотя бы ${FORGOTTEN_MIN_PLAYS} раза, но не включали дольше ${FORGOTTEN_AFTER_DAYS} дней`,
    // Most-loved first: the point is rediscovering what used to be on repeat
    select: (tracks, now) =>
      tracks
        .filter(
          (t) =>
            t.play_count >= FORGOTTEN_MIN_PLAYS &&
            t.last_played_at != null &&
            now - t.last_played_at > FORGOTTEN_AFTER_DAYS * DAY,
        )
        .sort((a, b) => b.play_count - a.play_count || byTitle(a, b))
        .slice(0, TOP_LIMIT),
  },
  {
    id: "never-played",
    name: "Ни разу не слушал",
    empty: "Вы послушали всю библиотеку",
    // Newest first: the most likely to be unheard on purpose are the old ones
    select: (tracks) =>
      tracks
        .filter((t) => t.play_count === 0)
        .sort((a, b) => b.added_at - a.added_at || byTitle(a, b)),
  },
  {
    id: "recently-added",
    name: "Добавлено за месяц",
    empty: `За последние ${RECENT_DAYS} дней ничего не добавлялось`,
    select: (tracks, now) =>
      tracks
        .filter((t) => now - t.added_at <= RECENT_DAYS * DAY)
        .sort((a, b) => b.added_at - a.added_at || byTitle(a, b)),
  },
];

export function smartPlaylist(id: SmartPlaylistId): SmartPlaylist {
  return SMART_PLAYLISTS.find((p) => p.id === id)!;
}
