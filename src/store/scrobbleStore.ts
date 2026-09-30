import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { create } from "zustand";
import { commands } from "../bindings";
import type { ScrobbleStatus } from "../types";

interface ScrobbleState {
  status: ScrobbleStatus | null;
  /** True between opening Last.fm's approval page and finishing the auth. */
  lastfmAwaitingApproval: boolean;
  init: () => Promise<void>;
  connectListenbrainz: (token: string) => Promise<void>;
  disconnectListenbrainz: () => Promise<void>;
  beginLastfm: (apiKey: string, secret: string) => Promise<void>;
  finishLastfm: () => Promise<void>;
  disconnectLastfm: () => Promise<void>;
}

let following = false;

export const useScrobbleStore = create<ScrobbleState>((set) => ({
  status: null,
  lastfmAwaitingApproval: false,
  init: async () => {
    set({ status: await commands.getScrobbleStatus() });
    // Queue lengths and errors change as the backend sends in the background
    if (!following) {
      following = true;
      void listen<ScrobbleStatus>("scrobble-status", (e) => set({ status: e.payload }));
    }
  },
  connectListenbrainz: async (token) => {
    set({ status: await commands.connectListenbrainz(token) });
  },
  disconnectListenbrainz: async () => {
    set({ status: await commands.disconnectListenbrainz() });
  },
  beginLastfm: async (apiKey, secret) => {
    const url = await commands.lastfmBeginAuth(apiKey, secret);
    await openUrl(url);
    set({ lastfmAwaitingApproval: true });
  },
  finishLastfm: async () => {
    const status = await commands.lastfmFinishAuth();
    set({ status, lastfmAwaitingApproval: false });
  },
  disconnectLastfm: async () => {
    set({ status: await commands.disconnectLastfm(), lastfmAwaitingApproval: false });
  },
}));
