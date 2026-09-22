import { useEffect, useState } from "react";
import type { LibrarySort } from "../lib/trackSort";

const STORAGE_KEY = "muzon:library-sort";
const VALID: LibrarySort[] = ["default", "recentlyAdded", "mostPlayed", "recentlyPlayed"];

function readStored(): LibrarySort {
  try {
    const stored = localStorage.getItem(STORAGE_KEY) as LibrarySort | null;
    return stored && VALID.includes(stored) ? stored : "default";
  } catch {
    return "default";
  }
}

export function useLibrarySort(): [LibrarySort, (sort: LibrarySort) => void] {
  const [sort, setSort] = useState<LibrarySort>(readStored);

  useEffect(() => {
    try {
      localStorage.setItem(STORAGE_KEY, sort);
    } catch {
      // per-viewer convenience only; fine if storage is unavailable
    }
  }, [sort]);

  return [sort, setSort];
}
