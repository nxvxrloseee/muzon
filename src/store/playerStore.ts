import { create } from "zustand";
import { playerApi } from "../api/player";
import { flushAllPendingPersists } from "../lib/debouncePersist";
import { useQueueStore } from "./queueStore";
import { useSleepTimerStore } from "./sleepTimerStore";
import type { PlaybackTick, Track } from "../types";

interface PlayerState {
  currentPath: string | null;
  isPlaying: boolean;
  /** Smoothly interpolated via rAF between ticks - what the UI should render. */
  positionSecs: number;
  durationSecs: number;
  volume: number;
  subscribed: boolean;
  /** Wall-clock baseline for interpolation: the position/timestamp pair from the
   * last authoritative tick (or seek), extrapolated forward each animation frame. */
  rawPositionSecs: number;
  rawPositionAt: number;
  play: (track: Track) => Promise<void>;
  toggle: () => Promise<void>;
  pause: () => Promise<void>;
  seek: (positionSecs: number) => Promise<void>;
  setVolume: (volume: number) => Promise<void>;
  stop: () => Promise<void>;
  ensureSubscribed: () => void;
}

export const usePlayerStore = create<PlayerState>((set, get) => ({
  currentPath: null,
  isPlaying: false,
  positionSecs: 0,
  durationSecs: 0,
  volume: 1,
  subscribed: false,
  rawPositionSecs: 0,
  rawPositionAt: performance.now(),
  play: async (track) => {
    // A pending debounced save (e.g. a tempo drag on the track we're leaving)
    // must land before we move on, or it's silently dropped - the timer would
    // still fire later, but by then the slider/track context has moved on and
    // nothing else guarantees it wasn't pre-empted by a later `schedule()`.
    await flushAllPendingPersists();
    await playerApi.playTrack(track.path);
    set({ currentPath: track.path });
  },
  toggle: async () => {
    await playerApi.togglePlay();
  },
  pause: async () => {
    await playerApi.pausePlayback();
    set({ isPlaying: false });
  },
  seek: async (positionSecs) => {
    await playerApi.seek(positionSecs);
    set({
      positionSecs,
      rawPositionSecs: positionSecs,
      rawPositionAt: performance.now(),
    });
  },
  setVolume: async (volume) => {
    await playerApi.setVolume(volume);
    set({ volume });
  },
  stop: async () => {
    await playerApi.stopPlayback();
    set({
      currentPath: null,
      isPlaying: false,
      positionSecs: 0,
      rawPositionSecs: 0,
      rawPositionAt: performance.now(),
    });
  },
  ensureSubscribed: () => {
    if (get().subscribed) return;
    set({ subscribed: true });

    playerApi.subscribeTicks((tick: PlaybackTick) => {
      set({
        isPlaying: tick.is_playing,
        durationSecs: tick.duration_secs,
        currentPath: tick.path,
        rawPositionSecs: tick.position_secs,
        rawPositionAt: performance.now(),
      });
      if (tick.auto_advanced_to) {
        // Gapless/crossfade transition completed on its own - move the queue's
        // cursor to match without re-invoking play (audio never stopped).
        useQueueStore.getState().syncCursorToPath(tick.auto_advanced_to);
      }
      const sleepConsumedEnd = useSleepTimerStore.getState().onTick(tick);
      if (tick.ended && !sleepConsumedEnd) {
        useQueueStore.getState().playNext();
      }
    });

    function frame() {
      const state = get();
      const elapsedSecs = (performance.now() - state.rawPositionAt) / 1000;
      const extrapolated = state.isPlaying
        ? state.rawPositionSecs + elapsedSecs
        : state.rawPositionSecs;
      const clamped =
        state.durationSecs > 0
          ? Math.min(extrapolated, state.durationSecs)
          : extrapolated;
      if (clamped !== state.positionSecs) {
        set({ positionSecs: clamped });
      }
      requestAnimationFrame(frame);
    }
    requestAnimationFrame(frame);
  },
}));
