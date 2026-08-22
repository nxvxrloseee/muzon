import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect } from "react";
import { flushAllPendingPersists } from "../lib/debouncePersist";

/** Every debounced setting (EQ, theme, crossfade, per-track tempo, ...) only
 * writes to disk/DB up to 300ms after the last change - closing the window
 * before that timer fires would otherwise silently drop the last value.
 * Intercepts the close request, waits for every pending save to land, then
 * lets the window actually close. */
export function useFlushSettingsOnClose() {
  useEffect(() => {
    const appWindow = getCurrentWindow();
    const unlistenPromise = appWindow.onCloseRequested(async (event) => {
      event.preventDefault();
      await flushAllPendingPersists();
      await appWindow.destroy();
    });
    return () => {
      unlistenPromise.then((unlisten) => unlisten());
    };
  }, []);
}
