import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect } from "react";
import { flushAllPendingPersists } from "../lib/debouncePersist";
import { useAppearanceStore } from "../store/appearanceStore";
import { saveSessionNow } from "../store/sessionStore";

/** Everything that must be on disk before the window goes: every debounced
 * setting (EQ, theme, crossfade, per-track tempo, ...) still waiting on its
 * 300ms timer, and the playhead, which only reaches the backend's memory as it
 * moves - this is the write that makes reopening resume on the exact second. */
async function saveEverything() {
  await flushAllPendingPersists();
  await saveSessionNow();
}

/** Intercepts closing the window to save first. With close-to-tray on, the
 * window is only hidden and playback carries on; the tray's and MPRIS's
 * "quit" (`app-quit-requested`) always closes for good. */
export function useFlushSettingsOnClose() {
  useEffect(() => {
    const appWindow = getCurrentWindow();
    const unlistenClose = appWindow.onCloseRequested(async (event) => {
      event.preventDefault();
      await saveEverything();
      if (useAppearanceStore.getState().appearance?.closeToTray) {
        await appWindow.hide();
      } else {
        await appWindow.destroy();
      }
    });
    const unlistenQuit = listen("app-quit-requested", async () => {
      await saveEverything();
      await appWindow.destroy();
    });
    return () => {
      unlistenClose.then((unlisten) => unlisten());
      unlistenQuit.then((unlisten) => unlisten());
    };
  }, []);
}
