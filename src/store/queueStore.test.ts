import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Track } from "../types";

vi.mock("../api/player", () => ({
  playerApi: {
    playTrack: vi.fn().mockResolvedValue(undefined),
    togglePlay: vi.fn().mockResolvedValue(undefined),
    seek: vi.fn().mockResolvedValue(undefined),
    setVolume: vi.fn().mockResolvedValue(undefined),
    stopPlayback: vi.fn().mockResolvedValue(undefined),
    pausePlayback: vi.fn().mockResolvedValue(undefined),
    subscribeTicks: vi.fn(),
    setNextTrack: vi.fn().mockResolvedValue(undefined),
    getPlaybackSettings: vi.fn(),
    setCrossfadeSeconds: vi.fn().mockResolvedValue(undefined),
    saveCrossfadeSeconds: vi.fn().mockResolvedValue(undefined),
    setEqualizerBands: vi.fn().mockResolvedValue(undefined),
    previewTrackTempo: vi.fn().mockResolvedValue(undefined),
    setTrackTempo: vi.fn().mockResolvedValue(undefined),
  },
}));

const { useQueueStore } = await import("./queueStore");
const { usePlayerStore } = await import("./playerStore");
const { playerApi } = await import("../api/player");

function track(id: number, title = `Track ${id}`): Track {
  return {
    id,
    path: `/music/${id}.mp3`,
    title,
    artist: null,
    album: null,
    duration_secs: 180,
    track_no: null,
    is_favorite: false,
    tempo: 1,
    play_count: 0,
    last_played_at: null,
    added_at: 0,
  };
}

const TRACKS = [track(1), track(2), track(3), track(4)];

beforeEach(() => {
  useQueueStore.setState({
    queue: [],
    shuffleOrder: [],
    cursor: -1,
    shuffle: false,
    repeat: "off",
  });
  usePlayerStore.setState({ currentPath: null });
  vi.clearAllMocks();
});

describe("setQueue", () => {
  it("snapshots the track list and starts at the chosen track", async () => {
    await useQueueStore.getState().setQueue(TRACKS, TRACKS[1]);

    const state = useQueueStore.getState();
    expect(state.queue).toEqual(TRACKS);
    expect(state.cursor).toBe(1);
    expect(playerApi.playTrack).toHaveBeenCalledWith("/music/2.mp3");
    expect(usePlayerStore.getState().currentPath).toBe("/music/2.mp3");
  });

  it("arms the backend with the next sequential track", async () => {
    await useQueueStore.getState().setQueue(TRACKS, TRACKS[0]);
    expect(playerApi.setNextTrack).toHaveBeenCalledWith("/music/2.mp3");
  });

  it("falls back to index 0 if the start track isn't in the list", async () => {
    await useQueueStore.getState().setQueue(TRACKS, track(999));
    expect(useQueueStore.getState().cursor).toBe(0);
  });
});

describe("playNext", () => {
  it("advances sequentially through the queue", async () => {
    await useQueueStore.getState().setQueue(TRACKS, TRACKS[0]);
    await useQueueStore.getState().playNext();
    expect(useQueueStore.getState().cursor).toBe(1);
    expect(playerApi.playTrack).toHaveBeenLastCalledWith("/music/2.mp3");
  });

  it("stops playback at the end of the queue when repeat is off", async () => {
    await useQueueStore.getState().setQueue(TRACKS, TRACKS[3]);
    await useQueueStore.getState().playNext();
    expect(playerApi.stopPlayback).toHaveBeenCalledTimes(1);
    expect(useQueueStore.getState().cursor).toBe(3);
  });

  it("wraps around to the first track when repeat is 'all'", async () => {
    await useQueueStore.getState().setQueue(TRACKS, TRACKS[3]);
    useQueueStore.setState({ repeat: "all" });
    await useQueueStore.getState().playNext();
    expect(useQueueStore.getState().cursor).toBe(0);
    expect(playerApi.playTrack).toHaveBeenLastCalledWith("/music/1.mp3");
  });

  it("replays the same track when repeat is 'one'", async () => {
    await useQueueStore.getState().setQueue(TRACKS, TRACKS[1]);
    useQueueStore.setState({ repeat: "one" });
    await useQueueStore.getState().playNext();
    expect(useQueueStore.getState().cursor).toBe(1);
    expect(playerApi.playTrack).toHaveBeenLastCalledWith("/music/2.mp3");
  });

  it("does nothing on an empty queue", async () => {
    await useQueueStore.getState().playNext();
    expect(playerApi.playTrack).not.toHaveBeenCalled();
    expect(playerApi.stopPlayback).not.toHaveBeenCalled();
  });
});

describe("playPrevious", () => {
  it("steps back sequentially", async () => {
    await useQueueStore.getState().setQueue(TRACKS, TRACKS[2]);
    await useQueueStore.getState().playPrevious();
    expect(useQueueStore.getState().cursor).toBe(1);
  });

  it("clamps at the first track", async () => {
    await useQueueStore.getState().setQueue(TRACKS, TRACKS[0]);
    await useQueueStore.getState().playPrevious();
    expect(useQueueStore.getState().cursor).toBe(0);
  });
});

describe("toggleShuffle", () => {
  it("does not regenerate the shuffle order or move the cursor", async () => {
    await useQueueStore.getState().setQueue(TRACKS, TRACKS[0]);
    const orderBefore = useQueueStore.getState().shuffleOrder;

    useQueueStore.getState().toggleShuffle();

    expect(useQueueStore.getState().shuffle).toBe(true);
    expect(useQueueStore.getState().shuffleOrder).toBe(orderBefore);
    expect(useQueueStore.getState().cursor).toBe(0);
  });
});

describe("cycleRepeat", () => {
  it("cycles off -> all -> one -> off", () => {
    expect(useQueueStore.getState().repeat).toBe("off");
    useQueueStore.getState().cycleRepeat();
    expect(useQueueStore.getState().repeat).toBe("all");
    useQueueStore.getState().cycleRepeat();
    expect(useQueueStore.getState().repeat).toBe("one");
    useQueueStore.getState().cycleRepeat();
    expect(useQueueStore.getState().repeat).toBe("off");
  });
});

describe("displayOrder", () => {
  it("rotates the queue so the current track is first, in sequential order", async () => {
    await useQueueStore.getState().setQueue(TRACKS, TRACKS[2]);
    const order = useQueueStore.getState().displayOrder();
    expect(order.map((t) => t.id)).toEqual([3, 4, 1, 2]);
  });

  it("returns an empty list when nothing is queued", () => {
    expect(useQueueStore.getState().displayOrder()).toEqual([]);
  });
});

describe("peekNextPath", () => {
  it("returns the next sequential path", async () => {
    await useQueueStore.getState().setQueue(TRACKS, TRACKS[0]);
    expect(useQueueStore.getState().peekNextPath()).toBe("/music/2.mp3");
  });

  it("returns null past the end with repeat off", async () => {
    await useQueueStore.getState().setQueue(TRACKS, TRACKS[3]);
    expect(useQueueStore.getState().peekNextPath()).toBeNull();
  });

  it("wraps to the first path with repeat 'all'", async () => {
    await useQueueStore.getState().setQueue(TRACKS, TRACKS[3]);
    useQueueStore.setState({ repeat: "all" });
    expect(useQueueStore.getState().peekNextPath()).toBe("/music/1.mp3");
  });

  it("returns the current path with repeat 'one'", async () => {
    await useQueueStore.getState().setQueue(TRACKS, TRACKS[1]);
    useQueueStore.setState({ repeat: "one" });
    expect(useQueueStore.getState().peekNextPath()).toBe("/music/2.mp3");
  });
});

describe("syncCursorToPath", () => {
  it("moves the cursor to match the reported path", async () => {
    await useQueueStore.getState().setQueue(TRACKS, TRACKS[0]);
    useQueueStore.getState().syncCursorToPath("/music/3.mp3");
    expect(useQueueStore.getState().cursor).toBe(2);
  });

  it("ignores paths not in the queue", async () => {
    await useQueueStore.getState().setQueue(TRACKS, TRACKS[0]);
    useQueueStore.getState().syncCursorToPath("/unknown.mp3");
    expect(useQueueStore.getState().cursor).toBe(0);
  });
});
