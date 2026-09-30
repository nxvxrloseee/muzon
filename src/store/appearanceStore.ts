import { create } from "zustand";
import { windowApi } from "../api/window";
import { createDebouncedPersist } from "../lib/debouncePersist";
import type { Appearance } from "../types";

const appearancePersist = createDebouncedPersist(300);

interface AppearanceState {
  appearance: Appearance | null;
  /** What this window was created with. `appearance.transparentWindow` may
   * already say otherwise, but that only applies after a restart. */
  windowTransparent: boolean;
  init: () => Promise<void>;
  setTransparentWindow: (on: boolean) => void;
  setBackgroundOpacity: (opacity: number) => void;
  setCloseToTray: (on: boolean) => void;
}

/** Opacity only means something on a window with an alpha channel; on an
 * opaque one a translucent background would just blend into black. */
function applyToDom(appearance: Appearance, windowTransparent: boolean) {
  const root = document.documentElement;
  const opacity = windowTransparent ? appearance.backgroundOpacity : 1;
  root.style.setProperty("--window-opacity", `${Math.round(opacity * 100)}%`);
  root.toggleAttribute("data-window-transparent", windowTransparent);
}

function persist(appearance: Appearance) {
  appearancePersist.schedule(() =>
    windowApi
      .setAppearance(appearance)
      .catch((e) => console.error("Failed to save appearance", e)),
  );
}

export const useAppearanceStore = create<AppearanceState>((set, get) => ({
  appearance: null,
  windowTransparent: false,
  init: async () => {
    const [appearance, windowTransparent] = await Promise.all([
      windowApi.getAppearance(),
      windowApi.isTransparent(),
    ]);
    applyToDom(appearance, windowTransparent);
    set({ appearance, windowTransparent });
  },
  setTransparentWindow: (on) => {
    const current = get().appearance;
    if (!current) return;
    // Nothing to apply now: the window keeps the alpha channel it was born with
    const next = { ...current, transparentWindow: on };
    set({ appearance: next });
    persist(next);
  },
  setCloseToTray: (on) => {
    const current = get().appearance;
    if (!current) return;
    const next = { ...current, closeToTray: on };
    set({ appearance: next });
    persist(next);
  },
  setBackgroundOpacity: (opacity) => {
    const current = get().appearance;
    if (!current) return;
    const next = { ...current, backgroundOpacity: opacity };
    applyToDom(next, get().windowTransparent);
    set({ appearance: next });
    persist(next);
  },
}));
