import { commands } from "../bindings";
import type { Appearance } from "../types";

export const windowApi = {
  getWindowControlsVisible: () => commands.getWindowControlsVisible(),
  getAppearance: () => commands.getAppearance(),
  setAppearance: (appearance: Appearance) => commands.setAppearance(appearance),
  isTransparent: () => commands.windowIsTransparent(),
};
