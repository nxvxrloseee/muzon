import { usePlayerStore } from "../store/playerStore";

function formatTime(secs: number): string {
  if (!Number.isFinite(secs) || secs <= 0) return "0:00";
  const m = Math.floor(secs / 60);
  const s = Math.floor(secs % 60);
  return `${m}:${s.toString().padStart(2, "0")}`;
}

/** Isolates the 60fps `positionSecs` subscription to just the elapsed/total
 * time labels and the seek slider, so it doesn't re-render the surrounding
 * playback controls on every animation frame. */
export function PlaybackProgress({
  startTimeClassName,
  endTimeClassName,
  inputClassName,
}: {
  startTimeClassName?: string;
  endTimeClassName?: string;
  inputClassName?: string;
}) {
  const positionSecs = usePlayerStore((s) => s.positionSecs);
  const durationSecs = usePlayerStore((s) => s.durationSecs);
  const seek = usePlayerStore((s) => s.seek);

  return (
    <>
      <span className={startTimeClassName}>{formatTime(positionSecs)}</span>
      <input
        type="range"
        min={0}
        max={durationSecs || 0}
        step={0.1}
        value={positionSecs}
        onChange={(e) => seek(Number(e.target.value))}
        className={inputClassName}
        style={{ accentColor: "var(--color-progress-fill)" }}
      />
      <span className={endTimeClassName}>{formatTime(durationSecs)}</span>
    </>
  );
}
