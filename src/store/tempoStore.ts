import { create } from "zustand";
import { playerApi } from "../api/player";
import { createDebouncedPersist } from "../lib/debouncePersist";
import type { Track } from "../types";

const tempoPersist = createDebouncedPersist(300);

interface TempoState {
  /** Per-track tempo the user has changed this session, keyed by track id.
   * Takes precedence over the value that came from the DB with the track. */
  overrides: Record<number, number>;
  setTempo: (track: Track, tempo: number) => void;
}

/**
 * Tempo used to live on the `Track` objects in `libraryStore`, so every event
 * of a slider drag rebuilt the whole tracks array - which re-ran the library
 * filter, the album/artist grouping and every list row for a value only the
 * effects panel reads. Keeping the dragged value here leaves the library
 * untouched until the debounced DB write lands.
 */
export const useTempoStore = create<TempoState>((set) => ({
  overrides: {},

  setTempo: (track, tempo) => {
    set((s) => ({ overrides: { ...s.overrides, [track.id]: tempo } }));

    // Live audio feedback is immediate and cheap (no disk I/O); the DB write
    // is debounced so dragging the slider doesn't write on every event.
    playerApi
      .previewTrackTempo(track.path, tempo)
      .catch((e) => console.error("Failed to preview tempo", e));

    tempoPersist.schedule(() =>
      playerApi.setTrackTempo(track.id, tempo).catch((e) => console.error("Failed to save tempo", e)),
    );
  },
}));

/** The tempo to show and apply for `track`: this session's override if the
 * user has touched the slider, otherwise whatever the DB loaded. */
export function useTrackTempo(track: Track | null): number {
  const override = useTempoStore((s) => (track ? s.overrides[track.id] : undefined));
  return override ?? track?.tempo ?? 1;
}
