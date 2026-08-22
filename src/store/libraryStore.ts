import { create } from "zustand";
import { toast } from "sonner";
import { libraryApi, type TrackEdit } from "../api/library";
import { playerApi } from "../api/player";
import { createDebouncedPersist } from "../lib/debouncePersist";
import type { Track } from "../types";

const tempoPersist = createDebouncedPersist(300);

interface LibraryState {
  tracks: Track[];
  scanning: boolean;
  error: string | null;
  refreshTracks: () => Promise<void>;
  addFolder: () => Promise<void>;
  updateTrackTags: (path: string, edit: TrackEdit) => Promise<void>;
  toggleFavorite: (trackId: number) => Promise<void>;
  setFavorite: (trackId: number, favorite: boolean) => Promise<void>;
  setTrackTempo: (trackId: number, tempo: number) => void;
}

export const useLibraryStore = create<LibraryState>((set, get) => ({
  tracks: [],
  scanning: false,
  error: null,
  refreshTracks: async () => {
    try {
      const tracks = await libraryApi.getTracks();
      set({ tracks, error: null });
    } catch (e) {
      set({ error: String(e) });
    }
  },
  addFolder: async () => {
    set({ scanning: true, error: null });
    try {
      const report = await libraryApi.addMusicFolder();
      await get().refreshTracks();
      if (report.errors.length > 0) {
        toast.warning(
          `Добавлено ${report.added}, обновлено ${report.updated}, ошибок: ${report.errors.length}`,
        );
      } else {
        toast.success(`Библиотека обновлена: +${report.added}, обновлено ${report.updated}`);
      }
    } catch (e) {
      set({ error: String(e) });
      toast.error(`Не удалось просканировать папку: ${String(e)}`);
    } finally {
      set({ scanning: false });
    }
  },
  updateTrackTags: async (path, edit) => {
    const updated = await libraryApi.updateTrackTags(path, edit);
    set((s) => ({
      tracks: s.tracks.map((t) => (t.path === path ? updated : t)),
    }));
  },
  toggleFavorite: async (trackId) => {
    // Optimistic flip so the heart responds instantly; reconciled with the
    // backend's actual result once the round trip completes.
    set((s) => ({
      tracks: s.tracks.map((t) =>
        t.id === trackId ? { ...t, is_favorite: !t.is_favorite } : t,
      ),
    }));
    const isFavorite = await libraryApi.toggleFavorite(trackId);
    set((s) => ({
      tracks: s.tracks.map((t) => (t.id === trackId ? { ...t, is_favorite: isFavorite } : t)),
    }));
  },
  setFavorite: async (trackId, favorite) => {
    const track = get().tracks.find((t) => t.id === trackId);
    if (!track || track.is_favorite === favorite) return;
    await get().toggleFavorite(trackId);
  },

  setTrackTempo: (trackId, tempo) => {
    const track = get().tracks.find((t) => t.id === trackId);
    if (!track) return;

    set((s) => ({
      tracks: s.tracks.map((t) => (t.id === trackId ? { ...t, tempo } : t)),
    }));
    // Live audio feedback is immediate and cheap (no disk I/O); the DB write
    // is debounced so dragging the slider doesn't write on every event.
    playerApi.previewTrackTempo(track.path, tempo).catch((e) => console.error("Failed to preview tempo", e));

    tempoPersist.schedule(() =>
      playerApi.setTrackTempo(trackId, tempo).catch((e) => console.error("Failed to save tempo", e)),
    );
  },
}));
