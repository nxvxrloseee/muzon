import { AnimatePresence, motion } from "motion/react";
import { ChevronDown, Music2, Pause, Pencil, Play } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { lyricsApi } from "../api/lyrics";
import { useCurrentTrack } from "../hooks/useCurrentTrack";
import { useTrackCover } from "../hooks/useTrackCover";
import { useTrackPalette, type TrackPalette } from "../hooks/useTrackPalette";
import { usePlayerStore } from "../store/playerStore";
import { useUiStore } from "../store/uiStore";
import type { Lyrics } from "../types";
import { LrcEditor } from "./LrcEditor";

function formatTime(secs: number): string {
  if (!Number.isFinite(secs) || secs <= 0) return "0:00";
  const m = Math.floor(secs / 60);
  const s = Math.floor(secs % 60);
  return `${m}:${s.toString().padStart(2, "0")}`;
}

function paletteGradient(p: TrackPalette): string {
  return [
    `radial-gradient(circle at 15% 20%, ${p.vibrant}b3 0%, transparent 55%)`,
    `radial-gradient(circle at 85% 15%, ${p.darkVibrant}b3 0%, transparent 60%)`,
    `radial-gradient(circle at 30% 90%, ${p.muted}80 0%, transparent 65%)`,
    `radial-gradient(circle at 90% 85%, ${p.darkVibrant}66 0%, transparent 60%)`,
    p.darkMuted,
  ].join(", ");
}

export function NowPlaying() {
  const { currentPath, isPlaying, positionSecs, durationSecs, toggle, seek } =
    usePlayerStore();
  const setView = useUiStore((s) => s.setView);
  const cover = useTrackCover(currentPath);
  const palette = useTrackPalette(cover);
  const track = useCurrentTrack();
  const [lyrics, setLyrics] = useState<Lyrics | null>(null);
  const [editorOpen, setEditorOpen] = useState(false);
  const activeLineRef = useRef<HTMLButtonElement | null>(null);

  const reloadLyrics = useCallback((path: string) => {
    lyricsApi.getLyrics(path).then(setLyrics);
  }, []);

  useEffect(() => {
    setLyrics(null);
    if (!currentPath) return;
    let cancelled = false;
    lyricsApi.getLyrics(currentPath).then((l) => {
      if (!cancelled) setLyrics(l);
    });
    return () => {
      cancelled = true;
    };
  }, [currentPath]);

  // Two-layer ping-pong crossfade: swapping the gradient straight to the next
  // track's palette would pop instantly (and to nothing while the palette is
  // still being extracted). Instead we keep both the previous and next
  // gradient mounted and cross-transition their opacity - the same technique
  // used for cover art, just driven by extracted colors instead of a photo.
  const [bg, setBg] = useState<{
    layers: [TrackPalette | null, TrackPalette | null];
    active: 0 | 1;
  }>({ layers: [null, null], active: 0 });

  useEffect(() => {
    if (!palette) return;
    setBg((prev) => {
      if (prev.layers[prev.active] === palette) return prev;
      const nextActive: 0 | 1 = prev.active === 0 ? 1 : 0;
      const nextLayers: [TrackPalette | null, TrackPalette | null] = [...prev.layers];
      nextLayers[nextActive] = palette;
      return { layers: nextLayers, active: nextActive };
    });
  }, [palette]);

  const activeLineIndex = useMemo(() => {
    if (!lyrics || lyrics.lines.length === 0) return -1;
    let idx = -1;
    for (let i = 0; i < lyrics.lines.length; i++) {
      if (lyrics.lines[i].time_secs <= positionSecs) idx = i;
      else break;
    }
    return idx;
  }, [lyrics, positionSecs]);

  useEffect(() => {
    activeLineRef.current?.scrollIntoView({
      behavior: "smooth",
      block: "center",
    });
  }, [activeLineIndex]);

  const title = track?.title ?? (currentPath ? "Без названия" : "Ничего не играет");
  const artist = track?.artist ?? "";

  return (
    <div className="relative flex h-full flex-col overflow-hidden bg-background">
      {([0, 1] as const).map((i) => (
        <div
          key={i}
          className="absolute inset-0 blur-[110px] transition-opacity duration-[1200ms] ease-out"
          style={{
            background: bg.layers[i] ? paletteGradient(bg.layers[i]!) : undefined,
            opacity: i === bg.active && bg.layers[i] ? 1 : 0,
          }}
        />
      ))}
      <div className="absolute inset-0 bg-background/45" />

      <div className="relative z-10 flex h-full flex-col">
        <button
          onClick={() => setView("library")}
          className="m-4 flex w-fit items-center gap-2 rounded-full bg-card-background/60 px-3 py-1.5 text-sm text-text-primary backdrop-blur-lg"
        >
          <ChevronDown size={16} />
          Свернуть
        </button>

        <div className="flex flex-1 items-center justify-center gap-6 overflow-hidden px-10 pb-10">
          <div className="flex w-72 flex-shrink-0 flex-col items-center gap-4 rounded-3xl bg-card-background/25 p-6 shadow-lg backdrop-blur-2xl">
            <AnimatePresence mode="wait">
              <motion.div
                key={currentPath ?? "empty"}
                initial={{ opacity: 0, scale: 0.92 }}
                animate={{ opacity: 1, scale: 1 }}
                transition={{ type: "spring", stiffness: 260, damping: 24 }}
                className="flex aspect-square w-full items-center justify-center overflow-hidden rounded-xl bg-card-background shadow-lg"
              >
                {cover ? (
                  <img src={cover} alt="" className="h-full w-full object-cover" />
                ) : (
                  <Music2 size={48} className="text-text-secondary/40" />
                )}
              </motion.div>
            </AnimatePresence>
            <div className="text-center">
              <div className="text-lg font-semibold text-text-primary">{title}</div>
              {artist && <div className="text-sm text-text-secondary">{artist}</div>}
            </div>
            <motion.button
              onClick={() => toggle()}
              disabled={!currentPath}
              whileHover={{ scale: 1.05 }}
              whileTap={{ scale: 0.94 }}
              transition={{ type: "spring", stiffness: 400, damping: 20 }}
              className="flex h-12 w-12 items-center justify-center rounded-full bg-accent-primary text-white disabled:opacity-40"
            >
              {isPlaying ? <Pause size={20} /> : <Play size={20} />}
            </motion.button>
            <div className="flex w-full items-center gap-2 text-xs text-text-secondary">
              <span>{formatTime(positionSecs)}</span>
              <input
                type="range"
                min={0}
                max={durationSecs || 0}
                step={0.1}
                value={positionSecs}
                onChange={(e) => seek(Number(e.target.value))}
                className="flex-1"
                style={{ accentColor: "var(--color-progress-fill)" }}
              />
              <span>{formatTime(durationSecs)}</span>
            </div>
          </div>

          <div className="flex h-full max-w-xl flex-1 flex-col justify-center gap-3 overflow-y-auto rounded-3xl bg-card-background/15 px-8 py-10 text-center shadow-lg backdrop-blur-2xl">
            <button
              onClick={() => setEditorOpen(true)}
              disabled={!currentPath}
              className="mx-auto flex items-center gap-1.5 rounded-full bg-card-background/70 px-3 py-1 text-xs text-text-secondary backdrop-blur hover:text-text-primary disabled:opacity-40"
            >
              <Pencil size={12} />
              Редактировать текст
            </button>

            {!lyrics || lyrics.lines.length === 0 ? (
              <p className="text-text-secondary">
                Текст песни не найден (.lrc рядом с файлом)
              </p>
            ) : (
              lyrics.lines.map((line, i) => (
                <button
                  key={i}
                  ref={i === activeLineIndex ? activeLineRef : undefined}
                  onClick={() => seek(line.time_secs)}
                  className={`rounded px-2 py-0.5 text-lg transition-colors duration-300 hover:bg-card-hover/60 ${
                    i === activeLineIndex
                      ? "font-semibold text-karaoke-active-line"
                      : "text-karaoke-inactive-line"
                  }`}
                >
                  {line.words && line.words.length > 0
                    ? line.words.map((w, wi) => (
                        <span
                          key={wi}
                          className={
                            w.time_secs <= positionSecs
                              ? "text-karaoke-active-word-highlight"
                              : undefined
                          }
                        >
                          {w.text}
                        </span>
                      ))
                    : line.text}
                </button>
              ))
            )}
          </div>
        </div>
      </div>

      <LrcEditor
        track={track}
        open={editorOpen}
        onOpenChange={(nextOpen) => {
          setEditorOpen(nextOpen);
          if (!nextOpen && currentPath) reloadLyrics(currentPath);
        }}
      />
    </div>
  );
}
