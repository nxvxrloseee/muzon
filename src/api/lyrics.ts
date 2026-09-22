import { commands } from "../bindings";

export const lyricsApi = {
  getLyrics: (path: string) => commands.getLyrics(path),
  fetchOnlineLyrics: (path: string) => commands.fetchOnlineLyrics(path),
  getLyricsSourceText: (path: string) => commands.getLyricsSourceText(path),
  saveLyrics: (path: string, content: string, storeInTag: boolean) =>
    commands.saveLyrics(path, content, storeInTag),
};
