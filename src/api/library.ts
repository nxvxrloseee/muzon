import { commands } from "../bindings";

export interface TrackEdit {
  title: string;
  artist: string | null;
  album: string | null;
  trackNo: number | null;
  coverPath: string | null;
}

export const libraryApi = {
  addMusicFolder: () => commands.addMusicFolder(),
  getTracks: () => commands.getTracks(),
  getTrackCover: (path: string) => commands.getTrackCover(path),
  updateTrackTags: (path: string, edit: TrackEdit) =>
    commands.updateTrackTags(
      path,
      edit.title,
      edit.artist,
      edit.album,
      edit.trackNo,
      edit.coverPath,
    ),
  toggleFavorite: (trackId: number) => commands.toggleFavorite(trackId),
};
