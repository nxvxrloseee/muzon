import { useEffect, useRef } from "react";
import { playbackClock } from "../store/playbackClock";
import { usePlayerStore } from "../store/playerStore";

function formatTime(secs: number): string {
  if (!Number.isFinite(secs) || secs <= 0) return "0:00";
  const m = Math.floor(secs / 60);
  const s = Math.floor(secs % 60);
  return `${m}:${s.toString().padStart(2, "0")}`;
}

/**
 * The elapsed label and the seek slider are the only things that need the
 * position every frame, and neither needs React to produce it: the clock
 * subscription writes the slider value and the label text straight to the DOM.
 * That keeps playback at zero renders per second - previously this component
 * (and everything it sits inside) re-rendered ~60 times a second.
 */
export function PlaybackProgress({
  startTimeClassName,
  endTimeClassName,
  inputClassName,
}: {
  startTimeClassName?: string;
  endTimeClassName?: string;
  inputClassName?: string;
}) {
  const durationSecs = usePlayerStore((s) => s.durationSecs);
  const seek = usePlayerStore((s) => s.seek);

  const elapsedRef = useRef<HTMLSpanElement>(null);
  const sliderRef = useRef<HTMLInputElement>(null);
  // While the user drags the thumb, the clock must not fight them for the
  // slider's value; the seek on release re-syncs it.
  const draggingRef = useRef(false);

  useEffect(() => {
    let lastLabel = "";

    function render() {
      const positionSecs = playbackClock.get();
      if (!draggingRef.current && sliderRef.current) {
        sliderRef.current.value = String(positionSecs);
      }
      const label = formatTime(positionSecs);
      if (label !== lastLabel && elapsedRef.current) {
        lastLabel = label;
        elapsedRef.current.textContent = label;
      }
    }

    render();
    const unsubscribe = playbackClock.subscribe(render);

    function onPointerUp() {
      draggingRef.current = false;
    }
    window.addEventListener("pointerup", onPointerUp);

    return () => {
      unsubscribe();
      window.removeEventListener("pointerup", onPointerUp);
    };
  }, []);

  return (
    <>
      <span ref={elapsedRef} className={startTimeClassName}>
        {formatTime(playbackClock.get())}
      </span>
      <input
        ref={sliderRef}
        type="range"
        min={0}
        max={durationSecs || 0}
        step={0.1}
        defaultValue={playbackClock.get()}
        onPointerDown={() => {
          draggingRef.current = true;
        }}
        onChange={(e) => seek(Number(e.target.value))}
        className={inputClassName}
        style={{ accentColor: "var(--color-progress-fill)" }}
      />
      <span className={endTimeClassName}>{formatTime(durationSecs)}</span>
    </>
  );
}
