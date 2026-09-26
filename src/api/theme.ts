import { open, save } from "@tauri-apps/plugin-dialog";
import { commands } from "../bindings";
import type { Theme, ThemeSource } from "../types";

export const themeApi = {
  getTheme: () => commands.getTheme(),
  setTheme: (theme: Theme) => commands.setTheme(theme),
  getDefaultTheme: (mode: "dark" | "light") => commands.getDefaultTheme(mode),
  getSource: () => commands.getThemeSource(),
  setSource: (source: ThemeSource) => commands.setThemeSource(source),
  systemAvailable: () => commands.systemThemeAvailable(),

  async exportThemeToFile(): Promise<boolean> {
    const path = await save({
      defaultPath: "muzon-theme.json",
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (!path) return false;
    await commands.exportTheme(path);
    return true;
  },

  async importThemeFromFile(): Promise<Theme | null> {
    const path = await open({
      multiple: false,
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (!path || Array.isArray(path)) return null;
    return commands.importTheme(path);
  },
};
