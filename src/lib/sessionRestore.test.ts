import { describe, expect, it } from "vitest";
import { normalizeSession, rebuildQueue, type RestorableSession } from "./sessionRestore";
import type { Track } from "../types";

function track(id: number, path: string): Track {
  return {
    id,
    path,
    title: `Track ${id}`,
    artist: null,
    album: null,
    duration_secs: 100,
    track_no: null,
    is_favorite: false,
    tempo: 1,
    play_count: 0,
    last_played_at: null,
    added_at: 0,
  };
}

function session(overrides: Partial<RestorableSession> = {}): RestorableSession {
  return {
    queuePaths: [],
    shuffleOrder: [],
    cursor: -1,
    shuffle: false,
    repeat: "off",
    positionSecs: 0,
    volume: 1,
    ...overrides,
  };
}

const library = [track(1, "/a.mp3"), track(2, "/b.mp3"), track(3, "/c.mp3")];

describe("normalizeSession", () => {
  it("fills in every field a session file written by an older build omits", () => {
    expect(normalizeSession({})).toEqual({
      queuePaths: [],
      shuffleOrder: [],
      cursor: -1,
      shuffle: false,
      repeat: "off",
      positionSecs: 0,
      volume: 1,
    });
  });

  it("keeps values that are present, including falsy ones", () => {
    const normalized = normalizeSession({ volume: 0, cursor: 0, positionSecs: 0 });
    expect(normalized.volume).toBe(0);
    expect(normalized.cursor).toBe(0);
  });
});

describe("rebuildQueue", () => {
  it("restores the queue in its saved order, not library order", () => {
    const restored = rebuildQueue(
      session({ queuePaths: ["/c.mp3", "/a.mp3", "/b.mp3"], cursor: 1 }),
      library,
    );
    expect(restored?.queue.map((t) => t.path)).toEqual(["/c.mp3", "/a.mp3", "/b.mp3"]);
    expect(restored?.cursor).toBe(1);
  });

  it("re-indexes the cursor around tracks that left the library", () => {
    // "/a.mp3" is gone, so what was index 2 is now index 1 - resuming on the
    // saved index instead would come back on the wrong song.
    const restored = rebuildQueue(
      session({ queuePaths: ["/a.mp3", "/b.mp3", "/c.mp3"], cursor: 2 }),
      [library[1], library[2]],
    );
    expect(restored?.queue.map((t) => t.path)).toEqual(["/b.mp3", "/c.mp3"]);
    expect(restored?.cursor).toBe(1);
  });

  it("re-indexes the shuffle order around removed tracks", () => {
    const restored = rebuildQueue(
      session({ queuePaths: ["/a.mp3", "/b.mp3", "/c.mp3"], shuffleOrder: [2, 0, 1], cursor: 0 }),
      [library[0], library[2]],
    );
    // Old 2 -> new 1, old 0 -> new 0, old 1 dropped.
    expect(restored?.shuffleOrder).toEqual([1, 0]);
  });

  it("redraws a shuffle order that is no longer a permutation", () => {
    const restored = rebuildQueue(
      session({ queuePaths: ["/a.mp3", "/b.mp3", "/c.mp3"], shuffleOrder: [0, 0], cursor: 0 }),
      library,
    );
    expect([...(restored?.shuffleOrder ?? [])].sort()).toEqual([0, 1, 2]);
  });

  it("reports no cursor when the track that was playing is gone", () => {
    const restored = rebuildQueue(
      session({ queuePaths: ["/a.mp3", "/b.mp3"], cursor: 0 }),
      [library[1]],
    );
    expect(restored?.queue.map((t) => t.path)).toEqual(["/b.mp3"]);
    expect(restored?.cursor).toBe(-1);
  });

  it("returns null when nothing in the saved queue still exists", () => {
    expect(rebuildQueue(session({ queuePaths: ["/gone.mp3"], cursor: 0 }), library)).toBeNull();
    expect(rebuildQueue(session(), library)).toBeNull();
  });
});
