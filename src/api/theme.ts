import { open, save } from "@tauri-apps/plugin-dialog";
import { commands } from "../bindings";
import type { Theme } from "../types";

export const themeApi = {
  getTheme: () => commands.getTheme(),
  setTheme: (theme: Theme) => commands.setTheme(theme),
  getDefaultTheme: (mode: "dark" | "light") => commands.getDefaultTheme(mode),

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
