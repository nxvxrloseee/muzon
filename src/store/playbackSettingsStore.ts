import { create } from "zustand";
import { playerApi } from "../api/player";

export const EQ_BAND_COUNT = 10;
export const EQ_BAND_FREQS_HZ = [
  29, 59, 119, 237, 474, 947, 1889, 3770, 7523, 15011,
];

let persistTimer: ReturnType<typeof setTimeout> | null = null;
function schedulePersistEq(gains: number[]) {
  if (persistTimer) clearTimeout(persistTimer);
  persistTimer = setTimeout(() => {
    playerApi.setEqualizerBands(gains).catch((e) => console.error("Failed to save EQ", e));
  }, 300);
}

interface PlaybackSettingsState {
  crossfadeSecs: number;
  eqGains: number[];
  tempo: number;
  loaded: boolean;
  init: () => Promise<void>;
  setCrossfadeSecs: (secs: number) => Promise<void>;
  setEqGains: (gains: number[]) => void;
  setEqBand: (index: number, gain: number) => void;
  setTempo: (tempo: number) => Promise<void>;
}

export const usePlaybackSettingsStore = create<PlaybackSettingsState>((set, get) => ({
  crossfadeSecs: 0,
  eqGains: new Array(EQ_BAND_COUNT).fill(0),
  tempo: 1,
  loaded: false,

  init: async () => {
    const settings = await playerApi.getPlaybackSettings();
    set({
      crossfadeSecs: settings.crossfadeSecs,
      eqGains: [...settings.eqGains],
      tempo: settings.tempo,
      loaded: true,
    });
  },

  setCrossfadeSecs: async (secs) => {
    set({ crossfadeSecs: secs });
    await playerApi.setCrossfadeSeconds(secs);
  },

  setEqGains: (gains) => {
    set({ eqGains: gains });
    schedulePersistEq(gains);
  },

  setEqBand: (index, gain) => {
    const next = [...get().eqGains];
    next[index] = gain;
    set({ eqGains: next });
    schedulePersistEq(next);
  },

  setTempo: async (tempo) => {
    set({ tempo });
    await playerApi.setTempo(tempo);
  },
}));
