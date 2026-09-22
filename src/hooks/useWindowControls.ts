import { useEffect, useState } from "react";
import { windowApi } from "../api/window";

/** The answer depends only on the session the app was launched into, so it is
 * fetched once for the lifetime of the process rather than per component. */
let pending: Promise<boolean> | null = null;

/**
 * Whether this desktop expects the app to draw its own window buttons.
 *
 * Starts false and flips on once the backend answers: showing a titlebar for a
 * moment and then yanking it away would shift the whole layout on the desktops
 * that don't want one, which is the more jarring way round.
 */
export function useWindowControls(): boolean {
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    pending ??= windowApi.getWindowControlsVisible();
    let cancelled = false;
    pending
      .then((value) => {
        if (!cancelled) setVisible(value);
      })
      .catch((e) => console.error("Failed to detect window controls support", e));
    return () => {
      cancelled = true;
    };
  }, []);

  return visible;
}
