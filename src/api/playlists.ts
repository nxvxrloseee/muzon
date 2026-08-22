import { commands } from "../bindings";

export const playlistsApi = {
  getPlaylists: () => commands.getPlaylists(),
  createPlaylist: (name: string) => commands.createPlaylist(name),
  renamePlaylist: (id: number, name: string) => commands.renamePlaylist(id, name),
  deletePlaylist: (id: number) => commands.deletePlaylist(id),
  getPlaylistTracks: (id: number) => commands.getPlaylistTracks(id),
  addTrackToPlaylist: (playlistId: number, trackId: number) =>
    commands.addTrackToPlaylist(playlistId, trackId),
  removeTrackFromPlaylist: (playlistId: number, trackId: number) =>
    commands.removeTrackFromPlaylist(playlistId, trackId),
  reorderPlaylistTracks: (playlistId: number, trackIds: number[]) =>
    commands.reorderPlaylistTracks(playlistId, trackIds),
};
