import { create } from "zustand";
import { playerApi } from "../api/player";
import { flushAllPendingPersists } from "../lib/debouncePersist";
import { playbackClock } from "./playbackClock";
import { useQueueStore } from "./queueStore";
import { useSleepTimerStore } from "./sleepTimerStore";
import type { PlaybackTick, Track } from "../types";

interface PlayerState {
  currentPath: string | null;
  isPlaying: boolean;
  durationSecs: number;
  volume: number;
  subscribed: boolean;
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
  durationSecs: 0,
  volume: 1,
  subscribed: false,
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
    playbackClock.setPosition(positionSecs);
    await playerApi.seek(positionSecs);
  },
  setVolume: async (volume) => {
    await playerApi.setVolume(volume);
    set({ volume });
  },
  stop: async () => {
    await playerApi.stopPlayback();
    playbackClock.reset();
    set({ currentPath: null, isPlaying: false });
  },
  ensureSubscribed: () => {
    if (get().subscribed) return;
    set({ subscribed: true });

    playerApi.subscribeTicks((tick: PlaybackTick) => {
      playbackClock.applyTick(tick.position_secs, tick.is_playing, tick.duration_secs);

      // Ticks arrive 5x a second and almost always carry the same playing
      // state / duration / path as the last one. Writing them unconditionally
      // would wake every subscriber of this store on each tick for nothing.
      const prev = get();
      if (
        prev.isPlaying !== tick.is_playing ||
        prev.durationSecs !== tick.duration_secs ||
        prev.currentPath !== tick.path
      ) {
        set({
          isPlaying: tick.is_playing,
          durationSecs: tick.duration_secs,
          currentPath: tick.path,
        });
      }

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
  },
}));
