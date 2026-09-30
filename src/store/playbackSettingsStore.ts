import { create } from "zustand";
import { playerApi } from "../api/player";
import { createDebouncedPersist } from "../lib/debouncePersist";
import type { ReplayGainSettings } from "../types";

export const EQ_BAND_COUNT = 10;
export const EQ_BAND_FREQS_HZ = [
  29, 59, 119, 237, 474, 947, 1889, 3770, 7523, 15011,
];

const eqPersist = createDebouncedPersist(300);
const crossfadePersist = createDebouncedPersist(300);
const replayGainPersist = createDebouncedPersist(300);

export const REPLAY_GAIN_OFF: ReplayGainSettings = { mode: "off", preampDb: 0 };

interface PlaybackSettingsState {
  crossfadeSecs: number;
  eqGains: number[];
  replayGain: ReplayGainSettings;
  loaded: boolean;
  init: () => Promise<void>;
  setCrossfadeSecs: (secs: number) => void;
  setEqGains: (gains: number[]) => void;
  setEqBand: (index: number, gain: number) => void;
  setReplayGain: (settings: ReplayGainSettings) => void;
}

export const usePlaybackSettingsStore = create<PlaybackSettingsState>((set, get) => ({
  crossfadeSecs: 0,
  eqGains: new Array(EQ_BAND_COUNT).fill(0),
  replayGain: REPLAY_GAIN_OFF,
  loaded: false,

  init: async () => {
    const settings = await playerApi.getPlaybackSettings();
    set({
      crossfadeSecs: settings.crossfadeSecs,
      eqGains: [...settings.eqGains],
      replayGain: settings.replayGain ?? REPLAY_GAIN_OFF,
      loaded: true,
    });
  },

  setCrossfadeSecs: (secs) => {
    set({ crossfadeSecs: secs });
    // Cheap in-memory apply on every drag event; the disk write is debounced
    // separately so dragging the slider doesn't do a file write per event.
    playerApi.setCrossfadeSeconds(secs).catch((e) => console.error("Failed to apply crossfade", e));
    crossfadePersist.schedule(() =>
      playerApi.saveCrossfadeSeconds(secs).catch((e) => console.error("Failed to save crossfade", e)),
    );
  },

  setEqGains: (gains) => {
    set({ eqGains: gains });
    eqPersist.schedule(() =>
      playerApi.setEqualizerBands(gains).catch((e) => console.error("Failed to save EQ", e)),
    );
  },

  setEqBand: (index, gain) => {
    const next = [...get().eqGains];
    next[index] = gain;
    set({ eqGains: next });
    eqPersist.schedule(() =>
      playerApi.setEqualizerBands(next).catch((e) => console.error("Failed to save EQ", e)),
    );
  },

  setReplayGain: (settings) => {
    set({ replayGain: settings });
    // Applied and saved by the same command; debounced for the preamp slider
    replayGainPersist.schedule(() =>
      playerApi.setReplayGain(settings).catch((e) => console.error("Failed to apply ReplayGain", e)),
    );
  },
}));
