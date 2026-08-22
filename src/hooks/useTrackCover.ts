import { useEffect, useState } from "react";
import { libraryApi } from "../api/library";

const cache = new Map<string, string | null>();

export function useTrackCover(path: string | null | undefined): string | null {
  const [cover, setCover] = useState<string | null>(
    path && cache.has(path) ? cache.get(path)! : null,
  );

  useEffect(() => {
    if (!path) {
      setCover(null);
      return;
    }
    if (cache.has(path)) {
      setCover(cache.get(path)!);
      return;
    }
    let cancelled = false;
    libraryApi.getTrackCover(path).then((data) => {
      cache.set(path, data);
      if (!cancelled) setCover(data);
    });
    return () => {
      cancelled = true;
    };
  }, [path]);

  return cover;
}
