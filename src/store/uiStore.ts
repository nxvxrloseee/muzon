import { create } from "zustand";

export type ViewName =
  | "library"
  | "albums"
  | "artists"
  | "playlists"
  | "settings"
  | "now-playing";

interface UiState {
  view: ViewName;
  setView: (view: ViewName) => void;
}

export const useUiStore = create<UiState>((set) => ({
  view: "library",
  setView: (view) => set({ view }),
}));
