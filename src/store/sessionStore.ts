import { sessionApi } from "../api/session";
import { createDebouncedPersist } from "../lib/debouncePersist";
import { normalizeSession, rebuildQueue } from "../lib/sessionRestore";
import { useLibraryStore } from "./libraryStore";
import { playbackClock } from "./playbackClock";
import { usePlayerStore } from "./playerStore";
import { useQueueStore } from "./queueStore";

/**
 * Keeps the previous run's queue and playhead across restarts.
 *
 * The session is split in two on purpose. The queue is one path per track and
 * changes only when playback starts somewhere new, so it goes over IPC on a
 * debounce; the playhead moves continuously, so it's pushed often but only into
 * the backend's *memory*. Writing to disk is a third, separate act - a slow
 * heartbeat in Rust plus one exact write when the window closes - which is what
 * keeps a large queue from being re-serialized every few seconds.
 */
const PROGRESS_INTERVAL_MS = 5000;

/** Replacing the queue sends a path per track, so it waits for the user to
 * settle rather than firing on every step through an album. */
const queuePersist = createDebouncedPersist(2000);

let restored = false;
let queueDirty = false;

function pushProgress(): Promise<void> {
  const { cursor, shuffle, repeat } = useQueueStore.getState();
  return sessionApi.setSessionProgress(
    cursor,
    playbackClock.get(),
    usePlayerStore.getState().volume,
    shuffle,
    repeat,
  );
}

async function pushQueue(): Promise<void> {
  const { queue, shuffleOrder } = useQueueStore.getState();
  await sessionApi.setSessionQueue(
    queue.map((track) => track.path),
    shuffleOrder,
  );
  queueDirty = false;
}

function pushProgressSoon() {
  void pushProgress().catch((e) => console.error("Failed to update session", e));
}

/**
 * Restores the previous run's queue and playhead. Must run *after* the library
 * has loaded: a session stores paths, and they can only be turned back into
 * tracks against what the library currently holds.
 */
export async function restoreSession(): Promise<void> {
  if (restored) return;
  restored = true;
  try {
    const session = normalizeSession(await sessionApi.getSession());
    // The backend applied the saved volume to the pipeline at startup already;
    // this is only what puts the slider where it belongs.
    usePlayerStore.setState({ volume: session.volume });

    const rebuilt = rebuildQueue(session, useLibraryStore.getState().tracks);
    if (!rebuilt) return;
    await useQueueStore.getState().restore({
      ...rebuilt,
      shuffle: session.shuffle,
      repeat: session.repeat,
      positionSecs: session.positionSecs,
    });
  } catch (e) {
    console.error("Failed to restore session", e);
  }
}

/** Keeps the backend's copy of the session in step with the stores. Returns the
 * teardown for the caller's effect. */
export function startSessionSync(): () => void {
  const unsubscribeQueue = useQueueStore.subscribe((state, previous) => {
    if (state.queue !== previous.queue || state.shuffleOrder !== previous.shuffleOrder) {
      queueDirty = true;
      queuePersist.schedule(() => saveSessionNow());
    }
    if (
      state.cursor !== previous.cursor ||
      state.shuffle !== previous.shuffle ||
      state.repeat !== previous.repeat
    ) {
      pushProgressSoon();
    }
  });

  const unsubscribeVolume = usePlayerStore.subscribe((state, previous) => {
    if (state.volume !== previous.volume) pushProgressSoon();
  });

  // Nothing else reports the playhead moving, so it gets its own slow poll.
  const progressTimer = setInterval(pushProgressSoon, PROGRESS_INTERVAL_MS);

  return () => {
    unsubscribeQueue();
    unsubscribeVolume();
    clearInterval(progressTimer);
  };
}

/**
 * Pushes everything and writes it out. This is the call that guarantees the
 * exact closing state is kept; between it and the backend's heartbeat, an
 * unclean exit loses at most a minute of playhead.
 */
export async function saveSessionNow(): Promise<void> {
  try {
    if (queueDirty) await pushQueue();
    await pushProgress();
    await sessionApi.saveSession();
  } catch (e) {
    console.error("Failed to save session", e);
  }
}
