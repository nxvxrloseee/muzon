import { create } from "zustand";
import { toast } from "sonner";
import { libraryApi, type TrackEdit } from "../api/library";
import type { Track } from "../types";

interface LibraryState {
  tracks: Track[];
  scanning: boolean;
  error: string | null;
  refreshTracks: () => Promise<void>;
  addFolder: () => Promise<void>;
  updateTrackTags: (path: string, edit: TrackEdit) => Promise<void>;
  toggleFavorite: (trackId: number) => Promise<void>;
  /** A favourite already flipped elsewhere (the control socket): just show it. */
  applyFavorite: (trackId: number, isFavorite: boolean) => void;
  setFavorite: (trackId: number, favorite: boolean) => Promise<void>;
  /** Counts one listen. The local copy is bumped straight away so a sort by
   * play count reorders as you listen, without re-reading the whole library. */
  recordPlay: (trackId: number) => Promise<void>;
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
  applyFavorite: (trackId, isFavorite) => {
    set((s) => ({
      tracks: s.tracks.map((t) => (t.id === trackId ? { ...t, is_favorite: isFavorite } : t)),
    }));
  },
  recordPlay: async (trackId) => {
    const playedAt = Math.floor(Date.now() / 1000);
    set((s) => ({
      tracks: s.tracks.map((t) =>
        t.id === trackId
          ? { ...t, play_count: t.play_count + 1, last_played_at: playedAt }
          : t,
      ),
    }));
    await libraryApi
      .recordPlay(trackId)
      .catch((e) => console.error("Failed to record play", e));
  },
  setFavorite: async (trackId, favorite) => {
    const track = get().tracks.find((t) => t.id === trackId);
    if (!track || track.is_favorite === favorite) return;
    await get().toggleFavorite(trackId);
  },
}));
