import { commands } from "../bindings";
import type { Hotkeys } from "../types";

export const hotkeysApi = {
  getHotkeys: () => commands.getHotkeys(),
  setHotkeys: (hotkeys: Hotkeys) => commands.setHotkeys(hotkeys),
};
