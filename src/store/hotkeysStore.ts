import { create } from "zustand";
import { hotkeysApi } from "../api/hotkeys";
import type { Hotkeys } from "../types";

const FALLBACK: Hotkeys = {
  playPause: " ",
  seekForward: "ArrowRight",
  seekBackward: "ArrowLeft",
  volumeUp: "ArrowUp",
  volumeDown: "ArrowDown",
  nextTrack: "n",
  previousTrack: "p",
  closeNowPlaying: "Escape",
};

interface HotkeysState {
  bindings: Hotkeys;
  loaded: boolean;
  init: () => Promise<void>;
  setBinding: (action: keyof Hotkeys, key: string) => Promise<void>;
}

export const useHotkeysStore = create<HotkeysState>((set, get) => ({
  bindings: FALLBACK,
  loaded: false,
  init: async () => {
    const bindings = await hotkeysApi.getHotkeys();
    set({ bindings, loaded: true });
  },
  setBinding: async (action, key) => {
    const next = { ...get().bindings, [action]: key };
    set({ bindings: next });
    await hotkeysApi.setHotkeys(next);
  },
}));
