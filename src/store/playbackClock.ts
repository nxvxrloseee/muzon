import { useCallback, useSyncExternalStore } from "react";

/**
 * The playback position is the only piece of app state that changes every
 * frame. While it lived in `playerStore`, every `set` re-ran the selector of
 * *every* subscriber of that store - each visible track row, the player bar,
 * the queue drawer, every karaoke word - 60 times a second, even though those
 * components only care about `currentPath`/`isPlaying`.
 *
 * So the clock lives here instead, as a standalone external store. Nothing
 * subscribes to it except the components that actually render elapsed time, and
 * the interpolation loop only runs while something is both playing and watching.
 */

type Listener = () => void;

/** Wall-clock baseline: the position/timestamp pair from the last authoritative
 * backend tick (or seek), extrapolated forward from there. */
let rawPositionSecs = 0;
let rawPositionAt = performance.now();
let durationSecs = 0;
let playing = false;
let frameHandle: number | null = null;
const listeners = new Set<Listener>();

function currentPosition(): number {
  const elapsedSecs = playing ? (performance.now() - rawPositionAt) / 1000 : 0;
  const extrapolated = rawPositionSecs + elapsedSecs;
  return durationSecs > 0 ? Math.min(extrapolated, durationSecs) : extrapolated;
}

function emit() {
  for (const listener of listeners) listener();
}

function frame() {
  frameHandle = requestAnimationFrame(frame);
  emit();
}

/** Interpolating between ticks is only worth a frame loop when playback is
 * actually running *and* someone is rendering it; paused or backgrounded, the
 * loop stops entirely instead of spinning at 60fps to do nothing. */
function syncFrameLoop() {
  const shouldRun = playing && listeners.size > 0;
  if (shouldRun && frameHandle === null) {
    frameHandle = requestAnimationFrame(frame);
  } else if (!shouldRun && frameHandle !== null) {
    cancelAnimationFrame(frameHandle);
    frameHandle = null;
  }
}

export const playbackClock = {
  /** Always exact, whether or not the frame loop is running. */
  get: currentPosition,
  getDuration: () => durationSecs,

  subscribe(listener: Listener): () => void {
    listeners.add(listener);
    syncFrameLoop();
    return () => {
      listeners.delete(listener);
      syncFrameLoop();
    };
  },

  /** Authoritative position from the backend's 200ms tick. */
  applyTick(positionSecs: number, isPlaying: boolean, trackDurationSecs: number) {
    rawPositionSecs = positionSecs;
    rawPositionAt = performance.now();
    playing = isPlaying;
    durationSecs = trackDurationSecs;
    syncFrameLoop();
    emit();
  },

  /** Optimistic local jump (a seek), replaced by the next tick. */
  setPosition(positionSecs: number) {
    rawPositionSecs = positionSecs;
    rawPositionAt = performance.now();
    emit();
  },

  reset() {
    rawPositionSecs = 0;
    rawPositionAt = performance.now();
    durationSecs = 0;
    playing = false;
    syncFrameLoop();
    emit();
  },
};

/**
 * True while the position sits inside `[fromSecs, toSecs)`. Karaoke uses one of
 * these per word and per line: the component re-renders only on the frame its
 * own highlight actually flips, not on every frame.
 */
export function usePlaybackWithin(fromSecs: number, toSecs = Infinity): boolean {
  const getSnapshot = useCallback(
    () => currentPosition() >= fromSecs && currentPosition() < toSecs,
    [fromSecs, toSecs],
  );
  return useSyncExternalStore(playbackClock.subscribe, getSnapshot);
}

/**
 * Position quantized to `stepSecs`, so a component re-renders at the rate it
 * can actually display rather than at the rate the clock advances.
 */
export function usePlaybackPosition(stepSecs = 1): number {
  const getSnapshot = useCallback(
    () => Math.round(currentPosition() / stepSecs) * stepSecs,
    [stepSecs],
  );
  return useSyncExternalStore(playbackClock.subscribe, getSnapshot);
}
