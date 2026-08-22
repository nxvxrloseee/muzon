import { create } from "zustand";
import { themeApi } from "../api/theme";
import { applyThemeToDom } from "../theme/roles";
import type { Theme } from "../types";

let persistTimer: ReturnType<typeof setTimeout> | null = null;

function schedulePersist(theme: Theme) {
  if (persistTimer) clearTimeout(persistTimer);
  persistTimer = setTimeout(() => {
    themeApi.setTheme(theme).catch((e) => console.error("Failed to save theme", e));
  }, 300);
}

interface ThemeState {
  theme: Theme | null;
  init: () => Promise<void>;
  setColor: (key: keyof Theme, value: string) => void;
  resetToDefault: (mode: "dark" | "light") => Promise<void>;
  exportToFile: () => Promise<boolean>;
  importFromFile: () => Promise<boolean>;
}

export const useThemeStore = create<ThemeState>((set, get) => ({
  theme: null,
  init: async () => {
    const theme = await themeApi.getTheme();
    applyThemeToDom(theme);
    set({ theme });
  },
  setColor: (key, value) => {
    const current = get().theme;
    if (!current) return;
    const next = { ...current, [key]: value };
    applyThemeToDom(next);
    set({ theme: next });
    schedulePersist(next);
  },
  resetToDefault: async (mode) => {
    const theme = await themeApi.getDefaultTheme(mode);
    applyThemeToDom(theme);
    set({ theme });
    await themeApi.setTheme(theme);
  },
  exportToFile: () => themeApi.exportThemeToFile(),
  importFromFile: async () => {
    const theme = await themeApi.importThemeFromFile();
    if (!theme) return false;
    applyThemeToDom(theme);
    set({ theme });
    return true;
  },
}));
