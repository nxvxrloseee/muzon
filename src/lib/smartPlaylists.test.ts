import { describe, expect, it } from "vitest";
import { FORGOTTEN_AFTER_DAYS, smartPlaylist, TOP_LIMIT } from "./smartPlaylists";
import type { Track } from "../types";

const DAY = 24 * 60 * 60;
const NOW = 1_800_000_000;

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
    added_at: NOW - 365 * DAY,
    ...overrides,
  };
}

const titles = (tracks: Track[]) => tracks.map((t) => t.title);
const run = (id: Parameters<typeof smartPlaylist>[0], tracks: Track[]) =>
  titles(smartPlaylist(id).select(tracks, NOW));

describe("smart playlists", () => {
  it("favourites keep library order", () => {
    const tracks = [
      track({ title: "b", is_favorite: true }),
      track({ title: "x" }),
      track({ title: "a", is_favorite: true }),
    ];
    expect(run("favorites", tracks)).toEqual(["b", "a"]);
  });

  it("most played leaves out the unplayed and caps the list", () => {
    const many = Array.from({ length: TOP_LIMIT + 5 }, (_, i) =>
      track({ title: `t${i}`, play_count: i }),
    );
    const result = run("most-played", many);
    expect(result).toHaveLength(TOP_LIMIT);
    expect(result[0]).toBe(`t${TOP_LIMIT + 4}`);
    expect(result).not.toContain("t0");
  });

  it("recently played is newest first", () => {
    const tracks = [
      track({ title: "old", play_count: 1, last_played_at: NOW - 10 * DAY }),
      track({ title: "new", play_count: 1, last_played_at: NOW - DAY }),
      track({ title: "never" }),
    ];
    expect(run("recently-played", tracks)).toEqual(["new", "old"]);
  });

  it("forgotten wants tracks once on repeat and left alone since", () => {
    const long = NOW - (FORGOTTEN_AFTER_DAYS + 1) * DAY;
    const tracks = [
      track({ title: "loved-and-left", play_count: 9, last_played_at: long }),
      track({ title: "loved-less", play_count: 3, last_played_at: long }),
      track({ title: "tried-once", play_count: 1, last_played_at: long }),
      track({ title: "still-on", play_count: 9, last_played_at: NOW - DAY }),
    ];
    expect(run("forgotten", tracks)).toEqual(["loved-and-left", "loved-less"]);
  });

  it("never played is newest additions first", () => {
    const tracks = [
      track({ title: "older", added_at: NOW - 30 * DAY }),
      track({ title: "newer", added_at: NOW - DAY }),
      track({ title: "heard", play_count: 1 }),
    ];
    expect(run("never-played", tracks)).toEqual(["newer", "older"]);
  });

  it("recently added stops at a month", () => {
    const tracks = [
      track({ title: "week", added_at: NOW - 7 * DAY }),
      track({ title: "quarter", added_at: NOW - 90 * DAY }),
    ];
    expect(run("recently-added", tracks)).toEqual(["week"]);
  });

  it("never reorders the library it was given", () => {
    const tracks = [
      track({ title: "a", play_count: 1 }),
      track({ title: "b", play_count: 5 }),
    ];
    run("most-played", tracks);
    expect(titles(tracks)).toEqual(["a", "b"]);
  });
});
