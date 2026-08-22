import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { useQueueStore } from "../store/queueStore";

/** Next/Previous arrive here from the OS media-key/MPRIS surface (headphone
 * remote buttons, desktop media widgets) since queue order lives in the frontend. */
export function useMprisEvents() {
  useEffect(() => {
    const unlistenNext = listen("mpris-next", () => {
      useQueueStore.getState().playNext();
    });
    const unlistenPrevious = listen("mpris-previous", () => {
      useQueueStore.getState().playPrevious();
    });

    return () => {
      unlistenNext.then((f) => f());
      unlistenPrevious.then((f) => f());
    };
  }, []);
}
