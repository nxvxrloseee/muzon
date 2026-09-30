import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { toast } from "sonner";
import { libraryApi, type TrackEdit } from "../api/library";
import type { ScanReport, Track } from "../types";

interface LibraryState {
  tracks: Track[];
  /** The library folders, in the order they were added. */
  folders: string[];
  scanning: boolean;
  error: string | null;
  refreshTracks: () => Promise<void>;
  loadFolders: () => Promise<void>;
  addFolder: () => Promise<void>;
  /** Forgets the folder's tracks (the files stay). Resolves to how many. */
  removeFolder: (path: string) => Promise<number>;
  rescan: () => Promise<void>;
  updateTrackTags: (path: string, edit: TrackEdit) => Promise<void>;
  toggleFavorite: (trackId: number) => Promise<void>;
  /** A favourite already flipped elsewhere (the control socket): just show it. */
  applyFavorite: (trackId: number, isFavorite: boolean) => void;
  setFavorite: (trackId: number, favorite: boolean) => Promise<void>;
  /** Counts one listen. The local copy is bumped straight away so a sort by
   * play count reorders as you listen, without re-reading the whole library. */
  recordPlay: (trackId: number) => Promise<void>;
}

function reportScan(report: ScanReport, sayWhenNothingChanged = false) {
  const changed = report.added + report.updated + report.removed > 0;
  if (report.errors.length > 0) {
    toast.warning(
      `Добавлено ${report.added}, обновлено ${report.updated}, убрано ${report.removed}, ошибок: ${report.errors.length}`,
      { description: report.errors.slice(0, 3).join("\n") },
    );
  } else if (changed) {
    toast.success(
      `Библиотека обновлена: +${report.added}, обновлено ${report.updated}, убрано ${report.removed}`,
    );
  } else if (sayWhenNothingChanged) {
    toast.success("Изменений нет");
  }
}

/** The backend rescanned something (at startup, on a file change, after a
 * sync) and the library is different now. Returns the unsubscribe. */
export function followLibraryChanges(): () => void {
  const unlisten = listen<ScanReport>("library-changed", () => {
    void useLibraryStore.getState().refreshTracks();
  });
  return () => void unlisten.then((stop) => stop());
}

export const useLibraryStore = create<LibraryState>((set, get) => ({
  tracks: [],
  folders: [],
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
  loadFolders: async () => {
    set({ folders: await libraryApi.listMusicFolders() });
  },
  addFolder: async () => {
    set({ scanning: true, error: null });
    try {
      // The tracks themselves arrive through `library-changed`
      const report = await libraryApi.addMusicFolder();
      await get().loadFolders();
      reportScan(report);
    } catch (e) {
      set({ error: String(e) });
      toast.error(`Не удалось просканировать папку: ${String(e)}`);
    } finally {
      set({ scanning: false });
    }
  },
  removeFolder: async (path) => {
    const removed = await libraryApi.removeMusicFolder(path);
    await get().loadFolders();
    return removed;
  },
  rescan: async () => {
    set({ scanning: true, error: null });
    try {
      const report = await libraryApi.rescanLibrary();
      reportScan(report, true);
    } catch (e) {
      toast.error(`Не удалось пересканировать: ${String(e)}`);
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
