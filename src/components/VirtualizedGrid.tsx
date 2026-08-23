import { useVirtualizer } from "@tanstack/react-virtual";
import {
  Fragment,
  type Key,
  type ReactNode,
  type RefObject,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";

function useElementWidth(ref: RefObject<HTMLElement | null>): number {
  const [width, setWidth] = useState(0);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    setWidth(el.clientWidth);
    const observer = new ResizeObserver((entries) => {
      setWidth(entries[0].contentRect.width);
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, [ref]);

  return width;
}

interface VirtualizedGridProps<T> {
  items: T[];
  scrollElementRef: RefObject<HTMLDivElement | null>;
  /** Narrowest a column may get before the grid drops one. */
  minItemWidth: number;
  /** Row-height guess before a row has been measured. */
  estimateRowHeight: number;
  gap: number;
  getItemKey: (item: T, index: number) => Key;
  renderItem: (item: T, index: number) => ReactNode;
  overscan?: number;
  className?: string;
}

/**
 * Windowed card grid: items are chunked into rows of however many columns fit,
 * and only rows near the viewport are mounted. Shared by the library grid and
 * the album grid - the latter used to mount every album at once, each pulling
 * its own cover.
 */
export function VirtualizedGrid<T>({
  items,
  scrollElementRef,
  minItemWidth,
  estimateRowHeight,
  gap,
  getItemKey,
  renderItem,
  overscan = 4,
  className,
}: VirtualizedGridProps<T>) {
  const containerRef = useRef<HTMLDivElement>(null);
  const width = useElementWidth(containerRef);
  const columns = width > 0 ? Math.max(1, Math.floor((width + gap) / (minItemWidth + gap))) : 0;

  const rows = useMemo(() => {
    if (columns === 0) return [];
    const chunked: T[][] = [];
    for (let i = 0; i < items.length; i += columns) {
      chunked.push(items.slice(i, i + columns));
    }
    return chunked;
  }, [items, columns]);

  const rowVirtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollElementRef.current,
    estimateSize: () => estimateRowHeight,
    overscan,
  });

  return (
    <div ref={containerRef} className={className}>
      {columns > 0 && (
        <div style={{ position: "relative", height: rowVirtualizer.getTotalSize() }}>
          {rowVirtualizer.getVirtualItems().map((virtualRow) => {
            const row = rows[virtualRow.index];
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
                  paddingBottom: virtualRow.index < rows.length - 1 ? gap : 0,
                }}
              >
                <div
                  className="grid"
                  style={{
                    gap,
                    gridTemplateColumns: `repeat(${columns}, minmax(${minItemWidth}px, 1fr))`,
                  }}
                >
                  {/* Fragment rather than a wrapper element, so the rendered
                      card stays the direct grid child and keeps stretching to
                      the cell. */}
                  {row.map((item, columnIndex) => {
                    const index = virtualRow.index * columns + columnIndex;
                    return (
                      <Fragment key={getItemKey(item, index)}>{renderItem(item, index)}</Fragment>
                    );
                  })}
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
