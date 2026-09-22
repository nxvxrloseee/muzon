import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { useQueueStore } from "../store/queueStore";
import type { RepeatMode } from "../types";

/** Anything the OS media surface (headphone buttons, the desktop's media
 * widget) asks for that only the queue store can carry out, since queue order,
 * shuffle and repeat all live in the frontend. The backend answers the matching
 * MPRIS *reads* from the session it is kept supplied with. */
export function useMprisEvents() {
  useEffect(() => {
    const unlisteners = [
      listen("mpris-next", () => {
        useQueueStore.getState().playNext();
      }),
      listen("mpris-previous", () => {
        useQueueStore.getState().playPrevious();
      }),
      listen<boolean>("mpris-set-shuffle", (event) => {
        useQueueStore.getState().setShuffle(event.payload);
      }),
      listen<RepeatMode>("mpris-set-repeat", (event) => {
        useQueueStore.getState().setRepeat(event.payload);
      }),
    ];

    return () => {
      for (const unlisten of unlisteners) unlisten.then((f) => f());
    };
  }, []);
}
