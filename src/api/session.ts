import { commands } from "../bindings";
import type { RepeatMode } from "../types";

export const sessionApi = {
  getSession: () => commands.getSession(),
  /** The expensive half - one path per queued track. */
  setSessionQueue: (queuePaths: string[], shuffleOrder: number[]) =>
    commands.setSessionQueue(queuePaths, shuffleOrder),
  /** The cheap half, safe to call often: it only updates the backend's
   * in-memory copy, never the disk. */
  setSessionProgress: (
    cursor: number,
    positionSecs: number,
    volume: number,
    shuffle: boolean,
    repeat: RepeatMode,
  ) => commands.setSessionProgress(cursor, positionSecs, volume, shuffle, repeat),
  saveSession: () => commands.saveSession(),
};
