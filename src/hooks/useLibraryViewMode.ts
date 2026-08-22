import { useEffect, useState } from "react";

export type LibraryViewMode = "grid" | "list";

const STORAGE_KEY = "muzon:library-view-mode";

function readStored(): LibraryViewMode {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    return stored === "list" ? "list" : "grid";
  } catch {
    return "grid";
  }
}

export function useLibraryViewMode(): [LibraryViewMode, (mode: LibraryViewMode) => void] {
  const [mode, setMode] = useState<LibraryViewMode>(readStored);

  useEffect(() => {
    try {
      localStorage.setItem(STORAGE_KEY, mode);
    } catch {
      // per-viewer convenience only; fine if storage is unavailable
    }
  }, [mode]);

  return [mode, setMode];
}
