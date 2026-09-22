import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { useLibraryStore } from "../store/libraryStore";
import { useQueueStore } from "../store/queueStore";
import type { RepeatMode } from "../types";

/** Anything the OS media surface (headphone buttons, the desktop's media
 * widget) asks for that only the queue store can carry out, since queue order,
 * shuffle and repeat all live in the frontend. The backend answers the matching
 * MPRIS *reads* from the session it is kept supplied with.
 *
 * The control socket (data/control.rs) comes through here too: jumping to a
 * queue item, and favourites flipped from a desktop shell. */
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
      listen<number>("remote-play-queue-index", (event) => {
        useQueueStore.getState().playAtQueueIndex(event.payload);
      }),
      listen<{ trackId: number; isFavorite: boolean }>("remote-favorite-changed", (event) => {
        useLibraryStore.getState().applyFavorite(event.payload.trackId, event.payload.isFavorite);
      }),
    ];

    return () => {
      for (const unlisten of unlisteners) unlisten.then((f) => f());
    };
  }, []);
}
