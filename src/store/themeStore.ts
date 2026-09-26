import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { themeApi } from "../api/theme";
import { createDebouncedPersist } from "../lib/debouncePersist";
import { applyThemeToDom } from "../theme/roles";
import type { Theme, ThemeSource } from "../types";

const themePersist = createDebouncedPersist(300);

interface ThemeState {
  theme: Theme | null;
  /** "system" follows the desktop shell's palette, "manual" the user's own */
  source: ThemeSource;
  /** false when no shell palette was found - the switch is then pointless */
  systemAvailable: boolean;
  init: () => Promise<void>;
  setSource: (source: ThemeSource) => Promise<void>;
  setColor: (key: keyof Theme, value: string) => void;
  resetToDefault: (mode: "dark" | "light") => Promise<void>;
  exportToFile: () => Promise<boolean>;
  importFromFile: () => Promise<boolean>;
}

export const useThemeStore = create<ThemeState>((set, get) => ({
  theme: null,
  source: "manual",
  systemAvailable: false,
  init: async () => {
    const [theme, source, systemAvailable] = await Promise.all([
      themeApi.getTheme(),
      themeApi.getSource(),
      themeApi.systemAvailable(),
    ]);
    applyThemeToDom(theme);
    set({ theme, source, systemAvailable });

    // The shell repainted itself (new wallpaper): follow along
    listen<Theme>("theme-changed", (e) => {
      applyThemeToDom(e.payload);
      set({ theme: e.payload });
    });
  },
  setSource: async (source) => {
    const theme = await themeApi.setSource(source);
    applyThemeToDom(theme);
    set({ theme, source });
  },
  setColor: (key, value) => {
    const current = get().theme;
    if (!current) return;
    // Editing a colour leaves the system palette (the backend does the same)
    if (get().source === "system") set({ source: "manual" });
    const next = { ...current, [key]: value };
    applyThemeToDom(next);
    set({ theme: next });
    themePersist.schedule(() =>
      themeApi.setTheme(next).catch((e) => console.error("Failed to save theme", e)),
    );
  },
  resetToDefault: async (mode) => {
    const theme = await themeApi.getDefaultTheme(mode);
    applyThemeToDom(theme);
    set({ theme, source: "manual" });
    await themeApi.setTheme(theme);
  },
  exportToFile: () => themeApi.exportThemeToFile(),
  importFromFile: async () => {
    const theme = await themeApi.importThemeFromFile();
    if (!theme) return false;
    applyThemeToDom(theme);
    set({ theme, source: "manual" });
    return true;
  },
}));
