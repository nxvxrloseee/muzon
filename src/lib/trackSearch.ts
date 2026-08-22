import type { Track } from "../types";

export function trackMatchesQuery(track: Track, query: string): boolean {
  if (!query.trim()) return true;
  const q = query.trim().toLowerCase();
  return (
    track.title.toLowerCase().includes(q) ||
    (track.artist?.toLowerCase().includes(q) ?? false) ||
    (track.album?.toLowerCase().includes(q) ?? false)
  );
}
