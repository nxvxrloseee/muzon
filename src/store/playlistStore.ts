import { create } from "zustand";
import { toast } from "sonner";
import { playlistsApi } from "../api/playlists";
import type { Playlist, Track } from "../types";

interface PlaylistState {
  playlists: Playlist[];
  selectedId: number | null;
  tracks: Track[];
  loading: boolean;
  error: string | null;
  refreshPlaylists: () => Promise<void>;
  selectPlaylist: (id: number | null) => Promise<void>;
  createPlaylist: (name: string) => Promise<void>;
  renamePlaylist: (id: number, name: string) => Promise<void>;
  deletePlaylist: (id: number) => Promise<void>;
  addTrack: (playlistId: number, trackId: number) => Promise<void>;
  removeTrack: (playlistId: number, trackId: number) => Promise<void>;
  reorderTracks: (playlistId: number, orderedTrackIds: number[]) => Promise<void>;
}

export const usePlaylistStore = create<PlaylistState>((set, get) => ({
  playlists: [],
  selectedId: null,
  tracks: [],
  loading: false,
  error: null,

  refreshPlaylists: async () => {
    try {
      const playlists = await playlistsApi.getPlaylists();
      set({ playlists, error: null });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  selectPlaylist: async (id) => {
    set({ selectedId: id, loading: id !== null });
    if (id === null) {
      set({ tracks: [] });
      return;
    }
    try {
      const tracks = await playlistsApi.getPlaylistTracks(id);
      set({ tracks, error: null });
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ loading: false });
    }
  },

  createPlaylist: async (name) => {
    try {
      await playlistsApi.createPlaylist(name);
      await get().refreshPlaylists();
    } catch (e) {
      toast.error(`Не удалось создать плейлист: ${String(e)}`);
    }
  },

  renamePlaylist: async (id, name) => {
    try {
      await playlistsApi.renamePlaylist(id, name);
      await get().refreshPlaylists();
    } catch (e) {
      toast.error(`Не удалось переименовать плейлист: ${String(e)}`);
    }
  },

  deletePlaylist: async (id) => {
    try {
      await playlistsApi.deletePlaylist(id);
      if (get().selectedId === id) {
        set({ selectedId: null, tracks: [] });
      }
      await get().refreshPlaylists();
    } catch (e) {
      toast.error(`Не удалось удалить плейлист: ${String(e)}`);
    }
  },

  addTrack: async (playlistId, trackId) => {
    try {
      await playlistsApi.addTrackToPlaylist(playlistId, trackId);
      if (get().selectedId === playlistId) {
        await get().selectPlaylist(playlistId);
      }
      await get().refreshPlaylists();
    } catch (e) {
      toast.error(`Не удалось добавить трек в плейлист: ${String(e)}`);
    }
  },

  removeTrack: async (playlistId, trackId) => {
    set((s) => ({ tracks: s.tracks.filter((t) => t.id !== trackId) }));
    try {
      await playlistsApi.removeTrackFromPlaylist(playlistId, trackId);
      await get().refreshPlaylists();
    } catch (e) {
      toast.error(`Не удалось убрать трек: ${String(e)}`);
    }
  },

  reorderTracks: async (playlistId, orderedTrackIds) => {
    const byId = new Map(get().tracks.map((t) => [t.id, t]));
    const reordered = orderedTrackIds.map((id) => byId.get(id)).filter((t): t is Track => !!t);
    set({ tracks: reordered });
    try {
      await playlistsApi.reorderPlaylistTracks(playlistId, orderedTrackIds);
    } catch (e) {
      toast.error(`Не удалось сохранить порядок: ${String(e)}`);
    }
  },
}));
