import { Moon } from "lucide-react";
import { useState } from "react";
import { useSleepTimerStore } from "../store/sleepTimerStore";

const PRESETS_MIN = [5, 15, 30, 45, 60];

function formatRemaining(secs: number): string {
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
}

export function SleepTimerButton() {
  const [open, setOpen] = useState(false);
  const deadline = useSleepTimerStore((s) => s.deadline);
  const endOfTrackPending = useSleepTimerStore((s) => s.endOfTrackPending);
  const remainingSecs = useSleepTimerStore((s) => s.remainingSecs);
  const setMinutes = useSleepTimerStore((s) => s.setMinutes);
  const setEndOfTrack = useSleepTimerStore((s) => s.setEndOfTrack);
  const cancel = useSleepTimerStore((s) => s.cancel);
  const active = deadline !== null || endOfTrackPending;

  return (
    <div className="relative flex-shrink-0">
      <button
        onClick={() => setOpen((v) => !v)}
        title="Таймер сна"
        className={`flex h-9 w-9 items-center justify-center rounded-full ${
          active
            ? "bg-accent-primary/20 text-accent-primary"
            : "text-text-secondary hover:bg-card-hover"
        }`}
      >
        <Moon size={16} />
      </button>

      {open && (
        <div className="absolute bottom-full right-0 z-20 mb-2 w-52 rounded-md border border-divider bg-card-background p-2 shadow-lg">
          {active && (
            <div className="mb-2 px-2 text-xs text-text-secondary">
              {endOfTrackPending
                ? "Остановка по окончании трека"
                : `Осталось: ${formatRemaining(remainingSecs ?? 0)}`}
            </div>
          )}
          {PRESETS_MIN.map((m) => (
            <button
              key={m}
              onClick={() => {
                setMinutes(m);
                setOpen(false);
              }}
              className="block w-full rounded px-2 py-1.5 text-left text-sm text-text-primary hover:bg-card-hover"
            >
              {m} минут
            </button>
          ))}
          <button
            onClick={() => {
              setEndOfTrack();
              setOpen(false);
            }}
            className="block w-full rounded px-2 py-1.5 text-left text-sm text-text-primary hover:bg-card-hover"
          >
            До конца трека
          </button>
          {active && (
            <button
              onClick={() => {
                cancel();
                setOpen(false);
              }}
              className="mt-1 block w-full rounded px-2 py-1.5 text-left text-sm text-red-400 hover:bg-card-hover"
            >
              Отменить
            </button>
          )}
        </div>
      )}
    </div>
  );
}
