import { SlidersHorizontal } from "lucide-react";
import { useEffect, useState } from "react";
import { useCurrentTrack } from "../hooks/useCurrentTrack";
import {
  EQ_BAND_FREQS_HZ,
  usePlaybackSettingsStore,
} from "../store/playbackSettingsStore";
import { useTempoStore, useTrackTempo } from "../store/tempoStore";

const PRESETS: Record<string, number[]> = {
  Плоский: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
  Бас: [6, 5, 4, 2, 0, 0, 0, 0, 0, 0],
  Вокал: [-2, -1, 0, 2, 4, 4, 2, 0, -1, -2],
  Рок: [4, 3, 1, 0, -1, 0, 2, 3, 4, 4],
};

const EQ_MIN = -24;
const EQ_MAX = 12;

function formatHz(hz: number): string {
  return hz >= 1000 ? `${Math.round(hz / 1000)}к` : `${hz}`;
}

function pctFromBottom(gain: number): number {
  return ((gain - EQ_MIN) / (EQ_MAX - EQ_MIN)) * 100;
}

export function EffectsButton() {
  const [open, setOpen] = useState(false);
  const crossfadeSecs = usePlaybackSettingsStore((s) => s.crossfadeSecs);
  const eqGains = usePlaybackSettingsStore((s) => s.eqGains);
  const init = usePlaybackSettingsStore((s) => s.init);
  const setCrossfadeSecs = usePlaybackSettingsStore((s) => s.setCrossfadeSecs);
  const setEqGains = usePlaybackSettingsStore((s) => s.setEqGains);
  const setEqBand = usePlaybackSettingsStore((s) => s.setEqBand);
  const track = useCurrentTrack();
  const tempo = useTrackTempo(track);
  const setTempo = useTempoStore((s) => s.setTempo);

  useEffect(() => {
    init();
  }, [init]);

  return (
    <div className="relative flex-shrink-0">
      <button
        onClick={() => setOpen((v) => !v)}
        title="Эквалайзер и эффекты"
        className={`flex h-9 w-9 items-center justify-center rounded-full ${
          open ? "bg-accent-primary/20 text-accent-primary" : "text-text-secondary hover:bg-card-hover"
        }`}
      >
        <SlidersHorizontal size={16} />
      </button>

      {open && (
        <div className="absolute bottom-full right-0 z-20 mb-2 w-80 rounded-md border border-divider bg-card-background p-4 shadow-lg">
          <div className="mb-3 flex flex-wrap gap-2">
            {Object.entries(PRESETS).map(([name, gains]) => (
              <button
                key={name}
                onClick={() => setEqGains(gains)}
                className="rounded-md bg-card-hover px-2 py-1 text-xs text-text-primary hover:bg-accent-primary/20"
              >
                {name}
              </button>
            ))}
          </div>

          <div className="flex items-end justify-between gap-1">
            {eqGains.map((gain, i) => (
              <div key={i} className="flex flex-1 flex-col items-center gap-1">
                <span className="tabular-nums text-[9px] text-text-secondary">
                  {gain > 0 ? `+${gain}` : gain}
                </span>

                {/* Visible fill track behind an invisible, still-interactive
                    slider: WebKit's native range input never shows a filled
                    portion, so a bare rotated <input> gives no sense of value.
                    A native vertical input (writing-mode: vertical-lr) is also
                    unreliable for drag hit-testing - rotating a normal
                    horizontal input is the robust cross-browser trick, since
                    pointer hit-testing follows the visual transform. */}
                <div className="relative h-24 w-5 overflow-hidden rounded-full bg-card-hover">
                  <div
                    className="absolute inset-x-0 bottom-0 bg-accent-primary"
                    style={{ height: `${pctFromBottom(gain)}%` }}
                  />
                  <div
                    className="absolute inset-x-0 h-px bg-divider"
                    style={{ top: `${100 - pctFromBottom(0)}%` }}
                  />
                  <input
                    type="range"
                    min={EQ_MIN}
                    max={EQ_MAX}
                    step={0.5}
                    value={gain}
                    onChange={(e) => setEqBand(i, Number(e.target.value))}
                    className="absolute left-1/2 top-1/2 h-6 w-24 -translate-x-1/2 -translate-y-1/2 -rotate-90 cursor-pointer appearance-none opacity-0"
                  />
                </div>

                <span className="text-[9px] text-text-secondary">
                  {formatHz(EQ_BAND_FREQS_HZ[i])}
                </span>
              </div>
            ))}
          </div>

          <div className="mt-4 flex flex-col gap-3 border-t border-divider pt-3">
            <div>
              <div className="mb-1 flex justify-between text-xs text-text-secondary">
                <span>Скорость</span>
                <span>{tempo.toFixed(2)}x</span>
              </div>
              <input
                type="range"
                min={0.5}
                max={2}
                step={0.05}
                value={tempo}
                disabled={!track}
                onChange={(e) => track && setTempo(track, Number(e.target.value))}
                className="w-full disabled:opacity-40"
                style={{ accentColor: "var(--color-accent-primary)" }}
              />
            </div>

            <div>
              <div className="mb-1 flex justify-between text-xs text-text-secondary">
                <span>Кроссфейд</span>
                <span>{crossfadeSecs === 0 ? "Гэплесс" : `${crossfadeSecs.toFixed(1)} с`}</span>
              </div>
              <input
                type="range"
                min={0}
                max={12}
                step={0.5}
                value={crossfadeSecs}
                onChange={(e) => setCrossfadeSecs(Number(e.target.value))}
                className="w-full"
                style={{ accentColor: "var(--color-accent-primary)" }}
              />
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
