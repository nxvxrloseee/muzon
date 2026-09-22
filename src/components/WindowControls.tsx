import { getCurrentWindow } from "@tauri-apps/api/window";
import { Maximize2, Minimize2, Minus, X } from "lucide-react";
import { useEffect, useState } from "react";

const BUTTON =
  "flex h-7 w-7 items-center justify-center rounded-md text-text-secondary transition-colors hover:bg-card-hover hover:text-text-primary";

/**
 * The window's own minimise / maximise / close buttons, plus the region it can
 * be dragged by.
 *
 * The window is `decorations: false`, so no compositor titlebar exists. On a
 * desktop that expects an app to draw its own there was previously no way to
 * minimise or maximise it, and - more to the point - no way to move it at all.
 * Whether this is rendered is `useWindowControls`' call, not this component's.
 *
 * The container carries `data-tauri-drag-region`: Tauri matches that attribute
 * against the element a press actually landed on, so the buttons inside stay
 * clickable while the space around them drags the window (and double-clicking
 * it maximises, as a titlebar should).
 */
export function WindowControls({ className = "" }: { className?: string }) {
  const [maximized, setMaximized] = useState(false);
  const appWindow = getCurrentWindow();

  useEffect(() => {
    let cancelled = false;
    const sync = () => {
      void appWindow
        .isMaximized()
        .then((value) => {
          if (!cancelled) setMaximized(value);
        })
        .catch((e) => console.error("Failed to read window state", e));
    };
    sync();
    // Maximising can come from outside this component - a compositor keybind, a
    // double-click on the drag region - so the icon follows the window itself
    // rather than assuming our own click was the only cause.
    const unlistenPromise = appWindow.onResized(sync);
    return () => {
      cancelled = true;
      unlistenPromise.then((unlisten) => unlisten());
    };
  }, [appWindow]);

  return (
    <div data-tauri-drag-region className={`flex items-center gap-0.5 ${className}`}>
      <button onClick={() => appWindow.minimize()} className={BUTTON} title="Свернуть">
        <Minus size={14} />
      </button>
      <button
        onClick={() => appWindow.toggleMaximize()}
        className={BUTTON}
        title={maximized ? "Восстановить размер" : "Развернуть на весь экран"}
      >
        {maximized ? <Minimize2 size={13} /> : <Maximize2 size={13} />}
      </button>
      <button
        // `close`, not `destroy`: this is what raises the close request the app
        // intercepts to flush pending settings and write the session out.
        onClick={() => appWindow.close()}
        className={`${BUTTON} hover:bg-red-500 hover:text-white`}
        title="Закрыть"
      >
        <X size={15} />
      </button>
    </div>
  );
}
