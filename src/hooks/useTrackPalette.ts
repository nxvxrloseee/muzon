import { useEffect, useState } from "react";
import { libraryApi } from "../api/library";
import type { TrackPalette } from "../types";

export type { TrackPalette };

const cache = new Map<string, TrackPalette | null>();

/**
 * Dominant colors of a track's cover, for the Now Playing background gradient.
 *
 * Extraction used to run in the webview through `node-vibrant`, which decoded
 * the cover and quantized its pixels on the main thread on every track change -
 * a visible hitch on exactly the screen where the gradient is meant to fade in
 * smoothly. It now happens in Rust off the UI thread, reusing the thumbnail
 * already cached for the cover art. Cached per path so repeated plays don't
 * even make the round trip.
 */
export function useTrackPalette(path: string | null): TrackPalette | null {
  const [palette, setPalette] = useState<TrackPalette | null>(
    path ? (cache.get(path) ?? null) : null,
  );

  useEffect(() => {
    if (!path) {
      setPalette(null);
      return;
    }
    if (cache.has(path)) {
      setPalette(cache.get(path)!);
      return;
    }
    let cancelled = false;
    libraryApi
      .getTrackPalette(path)
      .then((result) => {
        cache.set(path, result);
        if (!cancelled) setPalette(result);
      })
      .catch(() => {
        if (!cancelled) setPalette(null);
      });
    return () => {
      cancelled = true;
    };
  }, [path]);

  return palette;
}
