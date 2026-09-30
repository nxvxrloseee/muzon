import { describe, expect, it } from "vitest";
import {
  groupAlbums,
  groupArtists,
  groupGenres,
  keepUnchangedGroups,
  UNKNOWN_GENRE,
} from "./trackGroups";
import type { Track } from "../types";

function track(overrides: Partial<Track> & { title: string }): Track {
  return {
    id: 0,
    path: `/${overrides.title}.mp3`,
    artist: null,
    album: null,
    duration_secs: 200,
    track_no: null,
    genre: null,
    year: null,
    is_favorite: false,
    tempo: 1,
    play_count: 0,
    last_played_at: null,
    added_at: 0,
    ...overrides,
  };
}

const library = [
  track({ id: 1, title: "a1", artist: "A", album: "X", track_no: 1 }),
  track({ id: 2, title: "a2", artist: "A", album: "X", track_no: 2 }),
  track({ id: 3, title: "b1", artist: "B", album: "Y", track_no: 1 }),
];
const albumKey = (g: { key: string }) => g.key;

/** What the library store does on a like: one track object replaced. */
function liked(tracks: Track[], id: number): Track[] {
  return tracks.map((t) => (t.id === id ? { ...t, is_favorite: true } : t));
}

describe("keepUnchangedGroups", () => {
  it("returns the previous array when regrouping changed nothing", () => {
    const prev = groupAlbums(library);
    expect(keepUnchangedGroups(prev, groupAlbums([...library]), albumKey)).toBe(prev);
  });

  it("replaces only the group holding the changed track", () => {
    const prev = groupAlbums(library);
    const next = keepUnchangedGroups(prev, groupAlbums(liked(library, 3)), albumKey);
    expect(next).not.toBe(prev);
    expect(next[0]).toBe(prev[0]);
    expect(next[1]).not.toBe(prev[1]);
    expect(next[1].tracks[0].is_favorite).toBe(true);
  });

  it("does not reuse a group whose membership changed", () => {
    const prev = groupArtists(library);
    const extra = track({ id: 4, title: "a3", artist: "A", album: "Z" });
    const next = keepUnchangedGroups(prev, groupArtists([...library, extra]), (g) => g.artist);
    expect(next[0]).not.toBe(prev[0]);
    expect(next[0].tracks).toHaveLength(3);
    expect(next[1]).toBe(prev[1]);
  });

  it("notices a group that disappeared even when the rest is unchanged", () => {
    const prev = groupAlbums(library);
    const next = keepUnchangedGroups(prev, groupAlbums(library.slice(0, 2)), albumKey);
    expect(next).not.toBe(prev);
    expect(next).toEqual([prev[0]]);
  });
});

describe("groupGenres", () => {
  it("puts untagged tracks last instead of in alphabetical order", () => {
    const groups = groupGenres([
      track({ id: 1, title: "x" }),
      track({ id: 2, title: "y", genre: "Rock" }),
      track({ id: 3, title: "z", genre: "Ambient" }),
    ]);
    expect(groups.map((g) => g.genre)).toEqual(["Ambient", "Rock", UNKNOWN_GENRE]);
  });
});

describe("years", () => {
  it("orders an artist's albums oldest first, undated ones last", () => {
    const groups = groupAlbums([
      track({ id: 1, title: "a", artist: "A", album: "Late", year: 2001 }),
      track({ id: 2, title: "b", artist: "A", album: "Undated" }),
      track({ id: 3, title: "c", artist: "A", album: "Early", year: 1990 }),
    ]);
    expect(groups.map((g) => [g.album, g.year])).toEqual([
      ["Early", 1990],
      ["Late", 2001],
      ["Undated", null],
    ]);
  });

  it("dates an album by its earliest tagged track", () => {
    const [group] = groupAlbums([
      track({ id: 1, title: "a", album: "X", year: 1999 }),
      track({ id: 2, title: "b", album: "X", year: 1997 }),
      track({ id: 3, title: "c", album: "X" }),
    ]);
    expect(group.year).toBe(1997);
  });
});
