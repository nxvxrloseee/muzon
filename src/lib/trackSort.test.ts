import { describe, expect, it } from "vitest";
import { sortTracks } from "./trackSort";
import type { Track } from "../types";

function track(overrides: Partial<Track> & { title: string }): Track {
  return {
    id: 0,
    path: `/${overrides.title}.mp3`,
    artist: null,
    album: null,
    duration_secs: 200,
    track_no: null,
    is_favorite: false,
    tempo: 1,
    play_count: 0,
    last_played_at: null,
    added_at: 0,
    ...overrides,
  };
}

const titles = (tracks: Track[]) => tracks.map((t) => t.title);

describe("sortTracks", () => {
  it("hands back the very same array for the default order", () => {
    // The library arrives sorted by artist/album already, and returning the
    // same reference is what keeps downstream memos from invalidating.
    const tracks = [track({ title: "b" }), track({ title: "a" })];
    expect(sortTracks(tracks, "default")).toBe(tracks);
  });

  it("puts the newest additions first", () => {
    const tracks = [
      track({ title: "old", added_at: 100 }),
      track({ title: "new", added_at: 300 }),
      track({ title: "middle", added_at: 200 }),
    ];
    expect(titles(sortTracks(tracks, "recentlyAdded"))).toEqual(["new", "middle", "old"]);
  });

  it("ranks by play count and keeps never-played tracks rather than hiding them", () => {
    const tracks = [
      track({ title: "unplayed" }),
      track({ title: "favourite", play_count: 12 }),
      track({ title: "occasional", play_count: 3 }),
    ];
    expect(titles(sortTracks(tracks, "mostPlayed"))).toEqual([
      "favourite",
      "occasional",
      "unplayed",
    ]);
  });

  it("sorts never-played tracks last when ordering by last listened", () => {
    const tracks = [
      track({ title: "never" }),
      track({ title: "yesterday", last_played_at: 100 }),
      track({ title: "today", last_played_at: 200 }),
    ];
    expect(titles(sortTracks(tracks, "recentlyPlayed"))).toEqual([
      "today",
      "yesterday",
      "never",
    ]);
  });

  it("breaks ties on title so the order does not wobble between renders", () => {
    const tracks = [
      track({ title: "c", added_at: 100 }),
      track({ title: "a", added_at: 100 }),
      track({ title: "b", added_at: 100 }),
    ];
    expect(titles(sortTracks(tracks, "recentlyAdded"))).toEqual(["a", "b", "c"]);
  });

  it("leaves the input array untouched", () => {
    const tracks = [track({ title: "b", added_at: 1 }), track({ title: "a", added_at: 2 })];
    sortTracks(tracks, "recentlyAdded");
    expect(titles(tracks)).toEqual(["b", "a"]);
  });
});
