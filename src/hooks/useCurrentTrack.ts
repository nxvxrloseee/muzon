import { useLibraryStore } from "../store/libraryStore";
import { usePlayerStore } from "../store/playerStore";
import type { Track } from "../types";

/** Resolves the currently playing path to its DB-backed metadata (title/artist/album)
 * instead of ever showing the raw file path. */
export function useCurrentTrack(): Track | null {
  const currentPath = usePlayerStore((s) => s.currentPath);
  const tracks = useLibraryStore((s) => s.tracks);
  if (!currentPath) return null;
  return tracks.find((t) => t.path === currentPath) ?? null;
}
