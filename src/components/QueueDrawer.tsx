import { Drawer } from "vaul";
import { GripVertical, ListOrdered, Repeat, Repeat1, Shuffle, X } from "lucide-react";
import { useMemo, useRef, useState } from "react";
import { identityIndices, rotateToStart } from "../lib/shuffle";
import { usePlayerStore } from "../store/playerStore";
import { useQueueStore } from "../store/queueStore";
import type { Track } from "../types";
import { TrackCover } from "./TrackCover";
import { VirtualizedList } from "./VirtualizedList";

interface QueueRowProps {
  track: Track;
  /** Position in the drawer's own rotated view, which is also how a drag and a
   * removal are addressed. */
  index: number;
  isActive: boolean;
  isDropTarget: boolean;
  onPlay: () => void;
  onRemove: () => void;
  onDragStart: () => void;
  onDragEnter: () => void;
  onDragEnd: () => void;
  onDrop: () => void;
}

function QueueRow({
  track,
  index,
  isActive,
  isDropTarget,
  onPlay,
  onRemove,
  onDragStart,
  onDragEnter,
  onDragEnd,
  onDrop,
}: QueueRowProps) {
  // The playing track is pinned to the top of this view by the rotation, so
  // dragging it would look like nothing happened. Everything below it moves.
  const draggable = index > 0;

  return (
    <div
      draggable={draggable}
      onDragStart={(e) => {
        // Firefox refuses to start a drag without payload, and WebKit wants a
        // type it recognises.
        e.dataTransfer.setData("text/plain", String(index));
        e.dataTransfer.effectAllowed = "move";
        onDragStart();
      }}
      onDragOver={(e) => e.preventDefault()}
      onDragEnter={onDragEnter}
      onDragEnd={onDragEnd}
      onDrop={(e) => {
        e.preventDefault();
        onDrop();
      }}
      className={`group flex items-center gap-2 rounded-md px-2 py-2 transition-colors ${
        isActive ? "bg-card-hover" : "hover:bg-card-hover"
      } ${isDropTarget ? "outline outline-1 outline-accent-primary" : ""}`}
    >
      <span
        className={`flex h-5 w-4 flex-shrink-0 items-center justify-center text-text-secondary ${
          draggable ? "cursor-grab opacity-0 group-hover:opacity-100" : "opacity-0"
        }`}
      >
        <GripVertical size={13} />
      </span>

      <button onClick={onPlay} className="flex min-w-0 flex-1 items-center gap-3 text-left">
        <TrackCover
          path={track.path}
          className="flex h-10 w-10 flex-shrink-0 items-center justify-center overflow-hidden rounded bg-card-background"
        />
        <div className="min-w-0">
          <div
            className={`truncate text-sm ${
              isActive ? "text-karaoke-active-word-highlight" : "text-text-primary"
            }`}
          >
            {track.title}
          </div>
          <div className="truncate text-xs text-text-secondary">
            {track.artist ?? "Неизвестный исполнитель"}
          </div>
        </div>
      </button>

      <button
        onClick={onRemove}
        title="Убрать из очереди"
        className="flex h-6 w-6 flex-shrink-0 items-center justify-center rounded text-text-secondary opacity-0 transition-opacity hover:bg-card-background hover:text-text-primary group-hover:opacity-100"
      >
        <X size={13} />
      </button>
    </div>
  );
}

export function QueueDrawer() {
  const [open, setOpen] = useState(false);
  const queue = useQueueStore((s) => s.queue);
  const shuffleOrder = useQueueStore((s) => s.shuffleOrder);
  const cursor = useQueueStore((s) => s.cursor);
  const shuffle = useQueueStore((s) => s.shuffle);
  const repeat = useQueueStore((s) => s.repeat);
  const toggleShuffle = useQueueStore((s) => s.toggleShuffle);
  const cycleRepeat = useQueueStore((s) => s.cycleRepeat);
  const playAtQueueIndex = useQueueStore((s) => s.playAtQueueIndex);
  const removeFromQueue = useQueueStore((s) => s.removeFromQueue);
  const moveInView = useQueueStore((s) => s.moveInView);
  const currentPath = usePlayerStore((s) => s.currentPath);
  const scrollWrapperRef = useRef<HTMLDivElement>(null);

  // Where a drag started and where it currently hovers, both as positions in
  // this view. Held here rather than per row so a row can tell whether it is
  // the one about to be displaced.
  const [dragFrom, setDragFrom] = useState<number | null>(null);
  const [dragOver, setDragOver] = useState<number | null>(null);

  const rotatedIndices = useMemo(() => {
    if (queue.length === 0) return [];
    const order = shuffle ? shuffleOrder : identityIndices(queue.length);
    return rotateToStart(order, cursor);
  }, [queue, shuffleOrder, shuffle, cursor]);

  const ordered = useMemo(
    () => rotatedIndices.map((i) => queue[i]).filter(Boolean),
    [rotatedIndices, queue],
  );

  function finishDrag() {
    if (dragFrom !== null && dragOver !== null && dragFrom !== dragOver) {
      moveInView(dragFrom, dragOver);
    }
    setDragFrom(null);
    setDragOver(null);
  }

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

            <div ref={scrollWrapperRef} className="flex-1 overflow-y-auto">
              {ordered.length === 0 ? (
                <p className="text-sm text-text-secondary">
                  Очередь пуста — начните воспроизведение трека из библиотеки
                </p>
              ) : (
                // Windowed like every other track list: a queue is as long as
                // whatever list it was started from, so an unvirtualized one
                // mounted a row (and requested a cover) per library track.
                // Reordering here is native HTML5 drag and drop rather than a
                // Motion `Reorder.Group`, precisely because it addresses rows by
                // index and so needs no unmounted sibling to be measurable.
                <VirtualizedList
                  items={ordered}
                  scrollElementRef={scrollWrapperRef}
                  estimateSize={56}
                  gap={4}
                  getItemKey={(track, i) => `${track.id}-${i}`}
                  renderItem={(track, i) => (
                    <QueueRow
                      track={track}
                      index={i}
                      isActive={rotatedIndices[i] === cursor && track.path === currentPath}
                      isDropTarget={dragFrom !== null && dragOver === i && dragFrom !== i}
                      onPlay={() => playAtQueueIndex(rotatedIndices[i])}
                      onRemove={() => removeFromQueue(rotatedIndices[i])}
                      onDragStart={() => setDragFrom(i)}
                      onDragEnter={() => setDragOver(i)}
                      onDragEnd={finishDrag}
                      onDrop={finishDrag}
                    />
                  )}
                />
              )}
            </div>
          </div>
        </Drawer.Content>
      </Drawer.Portal>
    </Drawer.Root>
  );
}
