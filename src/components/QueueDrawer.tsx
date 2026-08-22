import { Drawer } from "vaul";
import { ListOrdered, Music2, Repeat, Repeat1, Shuffle, X } from "lucide-react";
import { useMemo, useState } from "react";
import { useTrackCover } from "../hooks/useTrackCover";
import { usePlayerStore } from "../store/playerStore";
import { useQueueStore } from "../store/queueStore";
import type { Track } from "../types";

function identityIndices(length: number): number[] {
  return Array.from({ length }, (_, i) => i);
}

function rotateToStart(order: number[], startValue: number): number[] {
  const p = order.indexOf(startValue);
  if (p <= 0) return order;
  return [...order.slice(p), ...order.slice(0, p)];
}

function QueueRow({ track, isActive, onPlay }: { track: Track; isActive: boolean; onPlay: () => void }) {
  const cover = useTrackCover(track.path);

  return (
    <button
      onClick={onPlay}
      className={`flex w-full items-center gap-3 rounded-md px-3 py-2 text-left transition-colors ${
        isActive ? "bg-card-hover" : "hover:bg-card-hover"
      }`}
    >
      <div className="flex h-10 w-10 flex-shrink-0 items-center justify-center overflow-hidden rounded bg-card-background">
        {cover ? (
          <img src={cover} alt="" className="h-full w-full object-cover" />
        ) : (
          <Music2 size={16} className="text-text-secondary/40" />
        )}
      </div>
      <div className="min-w-0">
        <div
          className={`truncate text-sm ${isActive ? "text-karaoke-active-word-highlight" : "text-text-primary"}`}
        >
          {track.title}
        </div>
        <div className="truncate text-xs text-text-secondary">
          {track.artist ?? "Неизвестный исполнитель"}
        </div>
      </div>
    </button>
  );
}

export function QueueDrawer() {
  const [open, setOpen] = useState(false);
  const { queue, shuffleOrder, cursor, shuffle, repeat, toggleShuffle, cycleRepeat, playAtQueueIndex } =
    useQueueStore();
  const currentPath = usePlayerStore((s) => s.currentPath);

  const rotatedIndices = useMemo(() => {
    if (queue.length === 0) return [];
    const order = shuffle ? shuffleOrder : identityIndices(queue.length);
    return rotateToStart(order, cursor);
  }, [queue, shuffleOrder, shuffle, cursor]);

  const ordered = rotatedIndices.map((i) => queue[i]).filter(Boolean);

  const repeatIcon = repeat === "one" ? <Repeat1 size={16} /> : <Repeat size={16} />;
  const repeatLabel =
    repeat === "off" ? "Без повтора" : repeat === "all" ? "Повтор очереди" : "Повтор трека";

  return (
    <Drawer.Root open={open} onOpenChange={setOpen} direction="right">
      <Drawer.Trigger
        className="flex h-9 w-9 flex-shrink-0 items-center justify-center rounded-full text-text-secondary hover:bg-card-hover hover:text-text-primary"
        title="Очередь"
      >
        <ListOrdered size={16} />
      </Drawer.Trigger>
      <Drawer.Portal>
        <Drawer.Overlay className="fixed inset-0 z-40 bg-black/40" />
        <Drawer.Content
          className="fixed inset-y-2 right-2 z-50 flex w-[380px] max-w-[calc(100vw-1rem)] outline-none"
          style={{ "--initial-transform": "calc(100% + 8px)" } as React.CSSProperties}
        >
          <div className="flex h-full w-full flex-col rounded-xl bg-sidebar-background p-4 shadow-lg ring-1 ring-divider">
            <div className="mb-4 flex items-center justify-between">
              <Drawer.Title className="text-lg font-semibold text-text-primary">Очередь</Drawer.Title>
              <button
                onClick={() => setOpen(false)}
                className="flex h-7 w-7 items-center justify-center rounded-full text-text-secondary hover:bg-card-hover hover:text-text-primary"
              >
                <X size={14} />
              </button>
            </div>

            <div className="mb-3 flex gap-2">
              <button
                onClick={() => toggleShuffle()}
                className={`flex items-center gap-2 rounded-md px-3 py-1.5 text-xs ${
                  shuffle
                    ? "bg-accent-primary/20 text-accent-primary"
                    : "bg-card-background text-text-secondary hover:bg-card-hover"
                }`}
              >
                <Shuffle size={14} />
                Случайный
              </button>
              <button
                onClick={() => cycleRepeat()}
                className={`flex items-center gap-2 rounded-md px-3 py-1.5 text-xs ${
                  repeat !== "off"
                    ? "bg-accent-primary/20 text-accent-primary"
                    : "bg-card-background text-text-secondary hover:bg-card-hover"
                }`}
              >
                {repeatIcon}
                {repeatLabel}
              </button>
            </div>

            <div className="flex-1 overflow-y-auto">
              {ordered.length === 0 ? (
                <p className="text-sm text-text-secondary">
                  Очередь пуста — начните воспроизведение трека из библиотеки
                </p>
              ) : (
                <div className="flex flex-col gap-1">
                  {ordered.map((track, i) => (
                    <QueueRow
                      key={`${track.id}-${i}`}
                      track={track}
                      isActive={rotatedIndices[i] === cursor && track.path === currentPath}
                      onPlay={() => playAtQueueIndex(rotatedIndices[i])}
                    />
                  ))}
                </div>
              )}
            </div>
          </div>
        </Drawer.Content>
      </Drawer.Portal>
    </Drawer.Root>
  );
}
