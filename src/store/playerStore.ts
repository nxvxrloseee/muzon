import { create } from "zustand";
import { toast } from "sonner";
import { playerApi } from "../api/player";
import { flushAllPendingPersists } from "../lib/debouncePersist";
import { shouldCountPlay } from "../lib/playThreshold";
import { useLibraryStore } from "./libraryStore";
import { playbackClock } from "./playbackClock";
import { useQueueStore } from "./queueStore";
import { useSleepTimerStore } from "./sleepTimerStore";
import type { PlaybackError, PlaybackTick, Track } from "../types";

/**
 * How many failures in a row we skip through before giving up. A queue whose
 * files have all moved would otherwise have each error advance to the next
 * track and its error, walking the entire library at five errors a second.
 */
const MAX_CONSECUTIVE_FAILURES = 5;
let consecutiveFailures = 0;

function fileName(path: string | null): string {
  return path?.split("/").pop() ?? "трек";
}

let watchedPath: string | null = null;
/** Whether this listen has been observed *below* the threshold. That is what
 * separates a real listen from a session restored past the halfway mark: only a
 * crossing we actually watched happen counts. */
let seenBeforeThreshold = false;
let alreadyCounted = false;

/** Counts a listen once it has gone far enough in to mean anything. */
function recordPlayIfEarned(tick: PlaybackTick) {
  const path = tick.path;
  if (path !== watchedPath) {
    watchedPath = path;
    seenBeforeThreshold = false;
    alreadyCounted = false;
  }
  if (!path) return;

  if (!shouldCountPlay(tick.position_secs, tick.duration_secs)) {
    // Below the line. Being here is what arms the next crossing - and it
    // re-arms after a repeat, so a track played twice counts twice.
    seenBeforeThreshold = true;
    alreadyCounted = false;
    return;
  }

  if (!seenBeforeThreshold || alreadyCounted) return;
  const track = useLibraryStore.getState().tracks.find((t) => t.path === path);
  if (!track) return;
  alreadyCounted = true;
  void useLibraryStore.getState().recordPlay(track.id);
}

/**
 * A GStreamer failure used to be swallowed along with every other non-EOS bus
 * message, which left playback stalled on a missing codec or an unreadable file
 * with nothing said and the queue never advancing. Now it surfaces and, when it
 * killed the deck that was actually playing, moves on.
 */
function handlePlaybackError(error: PlaybackError, stop: () => Promise<void>) {
  if (!error.fatal) {
    // Only the pre-load died - the current track is still playing and the
    // backend has already dropped the broken pre-roll. Nothing to say yet; it
    // gets reported properly if the queue ever reaches it.
    console.error("Pre-load failed", error);
    return;
  }

  consecutiveFailures += 1;
  if (consecutiveFailures >= MAX_CONSECUTIVE_FAILURES) {
    consecutiveFailures = 0;
    toast.error(
      `Не удалось воспроизвести «${fileName(error.path)}». Воспроизведение остановлено.`,
    );
    void stop();
    return;
  }
  toast.error(`«${fileName(error.path)}»: ${error.message}`);
  void useQueueStore.getState().playNext();
}

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
  /** Loads `track` paused at `positionSecs`, for session restore. Unlike `play`
   * this never starts audio: coming back to the app should look exactly like
   * leaving it did, not start playing on its own. */
  restore: (track: Track, positionSecs: number) => Promise<void>;
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
  restore: async (track, positionSecs) => {
    await playerApi.restoreTrack(track.path, positionSecs);
    playbackClock.setPosition(positionSecs);
    set({ currentPath: track.path, isPlaying: false });
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

      // Anything that reaches audio clears the failure streak: the guard exists
      // to stop a run of broken files, not to remember one from an hour ago.
      if (tick.is_playing) consecutiveFailures = 0;
      if (tick.error) handlePlaybackError(tick.error, get().stop);
      recordPlayIfEarned(tick);

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
