import { Music2, Pause, Play, SkipBack, SkipForward } from "lucide-react";
import { motion } from "motion/react";
import { useCurrentTrack } from "../hooks/useCurrentTrack";
import { useTrackCover } from "../hooks/useTrackCover";
import { usePlayerStore } from "../store/playerStore";
import { useQueueStore } from "../store/queueStore";
import { useUiStore } from "../store/uiStore";
import { EffectsButton } from "./EffectsButton";
import { QueueDrawer } from "./QueueDrawer";
import { SleepTimerButton } from "./SleepTimerButton";

function formatTime(secs: number): string {
  if (!Number.isFinite(secs) || secs <= 0) return "0:00";
  const m = Math.floor(secs / 60);
  const s = Math.floor(secs % 60);
  return `${m}:${s.toString().padStart(2, "0")}`;
}

export function PlayerBar() {
  const { currentPath, isPlaying, positionSecs, durationSecs, volume, toggle, seek, setVolume } =
    usePlayerStore();
  const { playNext, playPrevious } = useQueueStore();
  const setView = useUiStore((s) => s.setView);
  const cover = useTrackCover(currentPath);
  const track = useCurrentTrack();

  const title = track?.title ?? (currentPath ? "Без названия" : "Ничего не играет");
  const artist = track?.artist ?? "";

  return (
    <div className="flex h-20 items-center gap-4 border-t border-divider bg-player-bar-background px-4">
      <button
        onClick={() => currentPath && setView("now-playing")}
        disabled={!currentPath}
        className="flex w-56 flex-shrink-0 items-center gap-3 overflow-hidden rounded-md p-1 text-left hover:bg-card-hover disabled:cursor-default disabled:hover:bg-transparent"
      >
        <div className="flex h-12 w-12 flex-shrink-0 items-center justify-center overflow-hidden rounded bg-card-background">
          {cover ? (
            <img src={cover} alt="" className="h-full w-full object-cover" />
          ) : (
            <Music2 size={18} className="text-text-secondary/40" />
          )}
        </div>
        <span className="min-w-0">
          <span className="block truncate text-sm text-text-primary">{title}</span>
          {artist && (
            <span className="block truncate text-xs text-text-secondary">{artist}</span>
          )}
        </span>
      </button>

      <motion.button
        onClick={() => playPrevious()}
        disabled={!currentPath}
        whileTap={{ scale: 0.9 }}
        transition={{ type: "spring", stiffness: 400, damping: 25 }}
        className="flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-full text-text-secondary hover:bg-card-hover hover:text-text-primary disabled:opacity-40"
      >
        <SkipBack size={16} />
      </motion.button>

      <motion.button
        onClick={() => toggle()}
        disabled={!currentPath}
        whileHover={{ scale: 1.05 }}
        whileTap={{ scale: 0.92 }}
        transition={{ type: "spring", stiffness: 400, damping: 25 }}
        className="flex h-9 w-9 flex-shrink-0 items-center justify-center rounded-full bg-card-background text-text-primary hover:bg-card-hover disabled:opacity-40"
      >
        {isPlaying ? <Pause size={16} /> : <Play size={16} />}
      </motion.button>

      <motion.button
        onClick={() => playNext()}
        disabled={!currentPath}
        whileTap={{ scale: 0.9 }}
        transition={{ type: "spring", stiffness: 400, damping: 25 }}
        className="flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-full text-text-secondary hover:bg-card-hover hover:text-text-primary disabled:opacity-40"
      >
        <SkipForward size={16} />
      </motion.button>

      <span className="w-10 flex-shrink-0 text-right text-xs text-text-secondary">
        {formatTime(positionSecs)}
      </span>
      <input
        type="range"
        min={0}
        max={durationSecs || 0}
        step={0.1}
        value={positionSecs}
        onChange={(e) => seek(Number(e.target.value))}
        className="min-w-0 flex-1"
        style={{ accentColor: "var(--color-progress-fill)" }}
      />
      <span className="w-10 flex-shrink-0 text-xs text-text-secondary">
        {formatTime(durationSecs)}
      </span>

      <input
        type="range"
        min={0}
        max={1}
        step={0.01}
        value={volume}
        onChange={(e) => setVolume(Number(e.target.value))}
        className="w-24 flex-shrink-0"
        style={{ accentColor: "var(--color-progress-fill)" }}
      />

      <EffectsButton />
      <SleepTimerButton />
      <QueueDrawer />
    </div>
  );
}
