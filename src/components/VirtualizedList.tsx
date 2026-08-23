import { useVirtualizer } from "@tanstack/react-virtual";
import { useLayoutEffect, useReducer, type Key, type ReactNode, type RefObject } from "react";

interface VirtualizedListProps<T> {
  items: T[];
  scrollElementRef: RefObject<HTMLDivElement | null>;
  /** Initial row-height guess before a row is actually measured. */
  estimateSize: number;
  getItemKey: (item: T, index: number) => Key;
  renderItem: (item: T, index: number) => ReactNode;
  /** Vertical space between rows, in px. */
  gap?: number;
  overscan?: number;
  /** Applied to the outermost wrapper - use it for padding around the list. */
  className?: string;
}

/** Windowed row list shared by every flat track list in the app (TrackList's
 * list mode, album/artist detail views, playlist favorites) - only rows within
 * the viewport (plus overscan) are ever mounted. Not usable with a Framer
 * Motion `Reorder.Group`: Reorder needs every sibling `Reorder.Item` mounted
 * simultaneously to measure drag targets, which windowing fundamentally
 * breaks, so drag-to-reorder lists intentionally stay unvirtualized. */
export function VirtualizedList<T>({
  items,
  scrollElementRef,
  estimateSize,
  getItemKey,
  renderItem,
  gap = 0,
  overscan = 8,
  className,
}: VirtualizedListProps<T>) {
  // `scrollElementRef.current` is still null while this first renders, so the
  // virtualizer has nothing to measure and reports a zero-height list. One
  // forced re-render after mount hands it the real element. (Previously this
  // happened to work only because every caller re-rendered for other reasons
  // shortly after mounting.)
  const [, remeasureAfterMount] = useReducer((n: number) => n + 1, 0);
  useLayoutEffect(remeasureAfterMount, []);

  const rowVirtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => scrollElementRef.current,
    estimateSize: () => estimateSize,
    overscan,
    getItemKey: (index) => getItemKey(items[index], index),
  });

  return (
    <div className={className}>
      <div style={{ position: "relative", height: rowVirtualizer.getTotalSize() }}>
        {rowVirtualizer.getVirtualItems().map((virtualRow) => {
          const item = items[virtualRow.index];
          return (
            <div
              key={virtualRow.key}
              ref={rowVirtualizer.measureElement}
              data-index={virtualRow.index}
              style={{
                position: "absolute",
                top: 0,
                left: 0,
                width: "100%",
                transform: `translateY(${virtualRow.start}px)`,
                paddingBottom: virtualRow.index < items.length - 1 ? gap : 0,
              }}
            >
              {renderItem(item, virtualRow.index)}
            </div>
          );
        })}
      </div>
    </div>
  );
}
