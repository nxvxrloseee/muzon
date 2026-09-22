import { AnimatePresence, motion } from "motion/react";
import { ChevronDown, CloudDownload, Pause, Pencil, Play } from "lucide-react";
import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";
import { lyricsApi } from "../api/lyrics";
import { useCurrentTrack } from "../hooks/useCurrentTrack";
import { useTrackPalette, type TrackPalette } from "../hooks/useTrackPalette";
import { usePlaybackWithin } from "../store/playbackClock";
import { usePlayerStore } from "../store/playerStore";
import { useUiStore } from "../store/uiStore";
import type { LrcLine, LrcWord, Lyrics } from "../types";
import { LrcEditor } from "./LrcEditor";
import { PlaybackProgress } from "./PlaybackProgress";
import { TrackCover } from "./TrackCover";

function paletteGradient(p: TrackPalette): string {
  return [
    `radial-gradient(circle at 15% 20%, ${p.vibrant}b3 0%, transparent 55%)`,
    `radial-gradient(circle at 85% 15%, ${p.darkVibrant}b3 0%, transparent 60%)`,
    `radial-gradient(circle at 30% 90%, ${p.muted}80 0%, transparent 65%)`,
    `radial-gradient(circle at 90% 85%, ${p.darkVibrant}66 0%, transparent 60%)`,
    p.darkMuted,
  ].join(", ");
}

interface TimedLine extends LrcLine {
  endTimeSecs: number;
}

/** Subscribes to the playback clock through a narrow boolean, so this one word
 * (not the whole lyrics panel) re-renders exactly when its highlighted state
 * actually flips. The clock is deliberately a separate store from
 * `playerStore`: hundreds of these would otherwise wake every player
 * subscriber in the app on every animation frame. */
function LyricWord({ word }: { word: LrcWord }) {
  const active = usePlaybackWithin(word.time_secs);
  return (
    <span className={active ? "text-karaoke-active-word-highlight" : undefined}>{word.text}</span>
  );
}

/** Same narrow-selector trick as `LyricWord`, one level up: only the line
 * whose active window actually changes re-renders, instead of the whole
 * lyrics list re-rendering on every animation frame. Also owns its own
 * scroll-into-view, firing only when *this* line becomes active. */
const LyricLineItem = memo(function LyricLineItem({
  line,
  onSeek,
}: {
  line: TimedLine;
  onSeek: (secs: number) => void;
}) {
  const isActive = usePlaybackWithin(line.time_secs, line.endTimeSecs);
  const ref = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    if (isActive) ref.current?.scrollIntoView({ behavior: "smooth", block: "center" });
  }, [isActive]);

  return (
    <button
      ref={ref}
      onClick={() => onSeek(line.time_secs)}
      className={`rounded px-2 py-0.5 text-lg transition-colors duration-300 hover:bg-card-hover/60 ${
        isActive ? "font-semibold text-karaoke-active-line" : "text-karaoke-inactive-line"
      }`}
    >
      {line.words && line.words.length > 0
        ? line.words.map((w, wi) => <LyricWord key={wi} word={w} />)
        : line.text}
    </button>
  );
});

function LyricsPanel({
  lyrics,
  onSeek,
  onFetch,
  fetching,
}: {
  lyrics: Lyrics | null;
  onSeek: (secs: number) => void;
  onFetch: () => void;
  fetching: boolean;
}) {
  const timedLines = useMemo<TimedLine[]>(() => {
    if (!lyrics) return [];
    return lyrics.lines.map((line, i) => ({
      ...line,
      endTimeSecs: lyrics.lines[i + 1]?.time_secs ?? Infinity,
    }));
  }, [lyrics]);

  if (!lyrics || lyrics.lines.length === 0) {
    return (
      <div className="flex flex-col items-center gap-3">
        <p className="text-text-secondary">Текст песни не найден (.lrc рядом с файлом)</p>
        <button
          onClick={onFetch}
          disabled={fetching}
          className="flex items-center gap-1.5 rounded-full bg-card-background/90 px-3 py-1.5 text-xs text-text-secondary hover:text-text-primary disabled:opacity-40"
        >
          <CloudDownload size={13} />
          {fetching ? "Ищем…" : "Найти в LRCLIB"}
        </button>
      </div>
    );
  }

  return (
    <>
      {timedLines.map((line, i) => (
        <LyricLineItem key={i} line={line} onSeek={onSeek} />
      ))}
    </>
  );
}

export function NowPlaying() {
  const currentPath = usePlayerStore((s) => s.currentPath);
  const isPlaying = usePlayerStore((s) => s.isPlaying);
  const toggle = usePlayerStore((s) => s.toggle);
  const seek = usePlayerStore((s) => s.seek);
  const setView = useUiStore((s) => s.setView);
  const palette = useTrackPalette(currentPath);
  const track = useCurrentTrack();
  const [lyrics, setLyrics] = useState<Lyrics | null>(null);
  const [editorOpen, setEditorOpen] = useState(false);
  const [fetchingLyrics, setFetchingLyrics] = useState(false);

  const reloadLyrics = useCallback((path: string) => {
    lyricsApi.getLyrics(path).then(setLyrics);
  }, []);

  // Saved next to the file rather than into the tag: fetched lyrics are a
  // guess, and a sidecar is the one that can be deleted without rewriting the
  // audio file itself.
  const fetchLyrics = useCallback(async () => {
    if (!currentPath) return;
    setFetchingLyrics(true);
    try {
      const found = await lyricsApi.fetchOnlineLyrics(currentPath);
      if (!found) {
        toast.info("В LRCLIB ничего подходящего не нашлось");
        return;
      }
      await lyricsApi.saveLyrics(currentPath, found, false);
      reloadLyrics(currentPath);
      toast.success("Текст найден и сохранён рядом с файлом");
    } catch (e) {
      toast.error(`Не удалось загрузить текст: ${String(e)}`);
    } finally {
      setFetchingLyrics(false);
    }
  }, [currentPath, reloadLyrics]);

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

  const title = track?.title ?? (currentPath ? "Без названия" : "Ничего не играет");
  const artist = track?.artist ?? "";

  return (
    <div className="relative flex h-full flex-col overflow-hidden bg-background">
      {/* A 110px blur across a full-screen layer makes WebKit allocate (and
          re-blur) a full-screen offscreen buffer. Blurring a layer rendered at
          1/5 scale and then scaling it up looks the same - the gradient's stops
          are percentage-based - for 1/25th of the pixels. */}
      <div className="pointer-events-none absolute inset-0 overflow-hidden" style={{ contain: "paint" }}>
        {([0, 1] as const).map((i) => (
          <div
            key={i}
            className="absolute left-0 top-0 h-[20%] w-[20%] origin-top-left blur-[22px] transition-opacity duration-[1200ms] ease-out"
            style={{
              transform: "scale(5)",
              willChange: "opacity",
              background: bg.layers[i] ? paletteGradient(bg.layers[i]!) : undefined,
              opacity: i === bg.active && bg.layers[i] ? 1 : 0,
            }}
          />
        ))}
      </div>
      <div className="absolute inset-0 bg-background/45" />

      <div className="relative z-10 flex h-full flex-col">
        <button
          onClick={() => setView("library")}
          className="m-4 flex w-fit items-center gap-2 rounded-full bg-card-background/80 px-3 py-1.5 text-sm text-text-primary"
        >
          <ChevronDown size={16} />
          Свернуть
        </button>

        <div className="flex flex-1 items-center justify-center gap-6 overflow-hidden px-10 pb-10">
          <div className="flex w-72 flex-shrink-0 flex-col items-center gap-4 rounded-3xl bg-card-background/80 p-6 shadow-lg">
            <AnimatePresence mode="wait">
              <motion.div
                key={currentPath ?? "empty"}
                initial={{ opacity: 0, scale: 0.92 }}
                animate={{ opacity: 1, scale: 1 }}
                transition={{ type: "spring", stiffness: 260, damping: 24 }}
                className="w-full"
              >
                <TrackCover
                  path={currentPath}
                  iconSize={48}
                  className="flex aspect-square w-full items-center justify-center overflow-hidden rounded-xl bg-card-background shadow-lg"
                />
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
              <PlaybackProgress inputClassName="flex-1" />
            </div>
          </div>

          <div className="flex h-full max-w-xl flex-1 flex-col justify-center gap-3 overflow-y-auto rounded-3xl bg-card-background/70 px-8 py-10 text-center shadow-lg">
            <button
              onClick={() => setEditorOpen(true)}
              disabled={!currentPath}
              className="mx-auto flex items-center gap-1.5 rounded-full bg-card-background/90 px-3 py-1 text-xs text-text-secondary hover:text-text-primary disabled:opacity-40"
            >
              <Pencil size={12} />
              Редактировать текст
            </button>

            <LyricsPanel
              lyrics={lyrics}
              onSeek={seek}
              onFetch={fetchLyrics}
              fetching={fetchingLyrics}
            />
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
