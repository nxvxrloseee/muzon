import { useEffect, useRef } from "react";
import { useHotkeysStore } from "../store/hotkeysStore";
import { usePlayerStore } from "../store/playerStore";
import { useQueueStore } from "../store/queueStore";
import { useUiStore } from "../store/uiStore";

function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || target.isContentEditable;
}

const SEEK_STEP_SECS = 5;
const VOLUME_STEP = 0.05;

/**
 * Global playback hotkeys, remappable via the hotkeys store. Player/UI state is read
 * imperatively (getState/subscribe) so the DOM listener isn't torn down and re-added
 * on every position tick; only rebinding the keys themselves re-registers it.
 */
export function useHotkeys() {
  const bindings = useHotkeysStore((s) => s.bindings);
  const playerRef = useRef(usePlayerStore.getState());
  const uiRef = useRef(useUiStore.getState());

  useEffect(() => usePlayerStore.subscribe((s) => {
    playerRef.current = s;
  }), []);
  useEffect(() => useUiStore.subscribe((s) => {
    uiRef.current = s;
  }), []);

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (isTypingTarget(e.target)) return;

      const player = playerRef.current;
      const ui = uiRef.current;
      const key = e.key;

      if (key === bindings.playPause) {
        e.preventDefault();
        if (player.currentPath) player.toggle();
      } else if (key === bindings.seekForward) {
        e.preventDefault();
        if (player.currentPath) player.seek(player.positionSecs + SEEK_STEP_SECS);
      } else if (key === bindings.seekBackward) {
        e.preventDefault();
        if (player.currentPath) {
          player.seek(Math.max(0, player.positionSecs - SEEK_STEP_SECS));
        }
      } else if (key === bindings.volumeUp) {
        e.preventDefault();
        player.setVolume(Math.min(1, player.volume + VOLUME_STEP));
      } else if (key === bindings.volumeDown) {
        e.preventDefault();
        player.setVolume(Math.max(0, player.volume - VOLUME_STEP));
      } else if (key === bindings.nextTrack) {
        e.preventDefault();
        useQueueStore.getState().playNext();
      } else if (key === bindings.previousTrack) {
        e.preventDefault();
        useQueueStore.getState().playPrevious();
      } else if (key === bindings.closeNowPlaying) {
        if (ui.view === "now-playing") ui.setView("library");
      }
    }

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [bindings]);
}
