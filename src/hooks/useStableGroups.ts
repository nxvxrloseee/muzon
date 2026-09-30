import { useMemo, useRef } from "react";
import { keepUnchangedGroups } from "../lib/trackGroups";
import type { Track } from "../types";

/** Groups the library, keeping the previous group objects wherever their tracks
 * didn't change - see `keepUnchangedGroups`. */
export function useStableGroups<G extends { tracks: Track[] }>(
  tracks: Track[],
  group: (tracks: Track[]) => G[],
  keyOf: (group: G) => string,
): G[] {
  const prev = useRef<G[]>([]);
  return useMemo(() => {
    prev.current = keepUnchangedGroups(prev.current, group(tracks), keyOf);
    return prev.current;
    // `group` and `keyOf` are module-level functions at every call site
  }, [tracks]);
}
