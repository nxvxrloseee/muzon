import { Vibrant } from "node-vibrant/browser";
import { useEffect, useState } from "react";

export interface TrackPalette {
  vibrant: string;
  darkVibrant: string;
  muted: string;
  darkMuted: string;
}

const cache = new Map<string, TrackPalette>();

/** Extracts dominant colors from a track's cover (data URL) for the Now Playing
 * background gradient, per track so repeated plays don't re-run extraction. */
export function useTrackPalette(coverDataUrl: string | null): TrackPalette | null {
  const [palette, setPalette] = useState<TrackPalette | null>(
    coverDataUrl ? (cache.get(coverDataUrl) ?? null) : null,
  );

  useEffect(() => {
    if (!coverDataUrl) {
      setPalette(null);
      return;
    }
    const cached = cache.get(coverDataUrl);
    if (cached) {
      setPalette(cached);
      return;
    }
    let cancelled = false;
    Vibrant.from(coverDataUrl)
      .getPalette()
      .then((p) => {
        const result: TrackPalette = {
          vibrant: p.Vibrant?.hex ?? "#7c5cff",
          darkVibrant: p.DarkVibrant?.hex ?? "#2a1f4d",
          muted: p.Muted?.hex ?? "#4a4560",
          darkMuted: p.DarkMuted?.hex ?? "#15121f",
        };
        cache.set(coverDataUrl, result);
        if (!cancelled) setPalette(result);
      })
      .catch(() => {
        if (!cancelled) setPalette(null);
      });
    return () => {
      cancelled = true;
    };
  }, [coverDataUrl]);

  return palette;
}
