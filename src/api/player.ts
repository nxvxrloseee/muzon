import { Channel } from "@tauri-apps/api/core";
import { commands } from "../bindings";
import type { PlaybackTick } from "../types";

export const playerApi = {
  playTrack: (path: string) => commands.playTrack(path),
  togglePlay: () => commands.togglePlay(),
  seek: (positionSecs: number) => commands.seek(positionSecs),
  setVolume: (volume: number) => commands.setVolume(volume),
  stopPlayback: () => commands.stopPlayback(),
  pausePlayback: () => commands.pausePlayback(),
  subscribeTicks: (onTick: (tick: PlaybackTick) => void) => {
    const channel = new Channel<PlaybackTick>();
    channel.onmessage = onTick;
    return commands.subscribePlaybackTicks(channel);
  },
  setNextTrack: (path: string | null) => commands.setNextTrack(path),
  getPlaybackSettings: () => commands.getPlaybackSettings(),
  setCrossfadeSeconds: (secs: number) => commands.setCrossfadeSeconds(secs),
  setEqualizerBands: (gains: number[]) =>
    commands.setEqualizerBands(gains as unknown as Parameters<typeof commands.setEqualizerBands>[0]),
  setTempo: (tempo: number) => commands.setTempo(tempo),
};
