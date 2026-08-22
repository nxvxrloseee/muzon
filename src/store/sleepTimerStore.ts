import { create } from "zustand";
import { usePlayerStore } from "./playerStore";
import type { PlaybackTick } from "../types";

interface SleepTimerState {
  /** Epoch ms deadline for a duration-based timer, or null when not set. */
  deadline: number | null;
  /** True when set to "stop after the current track finishes". */
  endOfTrackPending: boolean;
  remainingSecs: number | null;
  setMinutes: (minutes: number) => void;
  setEndOfTrack: () => void;
  cancel: () => void;
  /**
   * Called on every playback tick. Returns true if it consumed this tick's `ended`
   * pulse by pausing (so the caller must not also advance the queue for it).
   */
  onTick: (tick: PlaybackTick) => boolean;
}

export const useSleepTimerStore = create<SleepTimerState>((set, get) => ({
  deadline: null,
  endOfTrackPending: false,
  remainingSecs: null,

  setMinutes: (minutes) => {
    set({
      deadline: Date.now() + minutes * 60_000,
      endOfTrackPending: false,
      remainingSecs: Math.round(minutes * 60),
    });
  },

  setEndOfTrack: () => {
    set({ endOfTrackPending: true, deadline: null, remainingSecs: null });
  },

  cancel: () => {
    set({ deadline: null, endOfTrackPending: false, remainingSecs: null });
  },

  onTick: (tick) => {
    const { deadline, endOfTrackPending } = get();

    if (deadline !== null) {
      const remaining = Math.max(0, Math.round((deadline - Date.now()) / 1000));
      set({ remainingSecs: remaining });
      if (Date.now() >= deadline) {
        usePlayerStore.getState().pause();
        set({ deadline: null, remainingSecs: null });
      }
      return false;
    }

    if (endOfTrackPending && tick.ended) {
      usePlayerStore.getState().pause();
      set({ endOfTrackPending: false });
      return true;
    }

    return false;
  },
}));
