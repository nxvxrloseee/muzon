import { ChevronLeft } from "lucide-react";
import { memo, useCallback, useDeferredValue, useMemo, useRef, useState } from "react";
import { type AlbumGroup, groupAlbums } from "../lib/trackGroups";
import { useLibraryStore } from "../store/libraryStore";
import { usePlayerStore } from "../store/playerStore";
import { useQueueStore } from "../store/queueStore";
import { useSearchStore } from "../store/searchStore";
import { TrackCover } from "./TrackCover";
import { VirtualizedGrid } from "./VirtualizedGrid";
import { VirtualizedList } from "./VirtualizedList";

const AlbumCard = memo(function AlbumCard({
  group,
  onOpen,
}: {
  group: AlbumGroup;
  onOpen: (key: string) => void;
}) {
  return (
    // CSS hover/press instead of Motion springs: these are windowed rows now,
    // so each one that scrolls into view would start its own animation.
    <button
      onClick={() => onOpen(group.key)}
      className="flex w-full flex-col gap-2 rounded-lg p-3 text-left transition-[background-color,transform] duration-150 hover:scale-[1.02] hover:bg-card-hover active:scale-[0.98]"
    >
      <TrackCover
        path={group.tracks[0]?.path}
        iconSize={28}
        className="flex aspect-square w-full items-center justify-center overflow-hidden rounded-md bg-card-background shadow-md"
      />
      <div className="min-w-0">
        <div className="truncate text-sm font-medium text-text-primary">{group.album}</div>
        <div className="truncate text-xs text-text-secondary">{group.artist}</div>
      </div>
    </button>
  );
});

function AlbumDetail({ group, onBack }: { group: AlbumGroup; onBack: () => void }) {
  const setQueue = useQueueStore((s) => s.setQueue);
  const currentPath = usePlayerStore((s) => s.currentPath);
  const scrollWrapperRef = useRef<HTMLDivElement>(null);

  return (
    <div className="flex h-full flex-col">
      <div className="p-4 pb-0">
        <button
          onClick={onBack}
          className="mb-4 flex items-center gap-1 text-sm text-text-secondary hover:text-text-primary"
        >
          <ChevronLeft size={16} />
          Все альбомы
        </button>

        <div className="mb-6 flex items-end gap-4">
          <TrackCover
            path={group.tracks[0]?.path}
            iconSize={32}
            className="flex h-32 w-32 flex-shrink-0 items-center justify-center overflow-hidden rounded-md bg-card-background"
          />
          <div>
            <div className="text-xl font-semibold text-text-primary">{group.album}</div>
            <div className="text-sm text-text-secondary">{group.artist}</div>
          </div>
        </div>
      </div>

      <div ref={scrollWrapperRef} data-lenis-prevent className="flex-1 overflow-y-auto">
        <div>
          <VirtualizedList
            items={group.tracks}
            scrollElementRef={scrollWrapperRef}
            estimateSize={44}
            gap={4}
            overscan={8}
            className="px-4 pb-4"
            getItemKey={(t) => t.id}
            renderItem={(t) => (
              <button
                onClick={() => setQueue(group.tracks, t)}
                className={`flex items-center gap-3 rounded-md px-3 py-2 text-left ${
                  currentPath === t.path ? "bg-card-hover" : "hover:bg-card-hover"
                }`}
              >
                <span className="w-6 flex-shrink-0 text-right text-xs text-text-secondary">
                  {t.track_no ?? ""}
                </span>
                <span className="truncate text-sm text-text-primary">{t.title}</span>
              </button>
            )}
          />
        </div>
      </div>
    </div>
  );
}

export function AlbumsView() {
  const tracks = useLibraryStore((s) => s.tracks);
  const query = useSearchStore((s) => s.query);
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const scrollWrapperRef = useRef<HTMLDivElement>(null);
  const openAlbum = useCallback((key: string) => setSelectedKey(key), []);

  const groups = useMemo(() => groupAlbums(tracks), [tracks]);
  const selected = groups.find((g) => g.key === selectedKey) ?? null;

  const deferredQuery = useDeferredValue(query);
  const filteredGroups = useMemo(() => {
    const q = deferredQuery.trim().toLowerCase();
    if (!q) return groups;
    return groups.filter(
      (g) => g.album.toLowerCase().includes(q) || g.artist.toLowerCase().includes(q),
    );
  }, [groups, deferredQuery]);

  if (tracks.length === 0) {
    return (
      <div className="flex h-full items-center justify-center text-text-secondary">
        Библиотека пуста — добавьте папку с музыкой
      </div>
    );
  }

  if (selected) {
    return <AlbumDetail group={selected} onBack={() => setSelectedKey(null)} />;
  }

  if (filteredGroups.length === 0) {
    return (
      <div className="flex h-full items-center justify-center text-text-secondary">
        Ничего не найдено
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <div ref={scrollWrapperRef} data-lenis-prevent className="flex-1 overflow-y-auto">
        <VirtualizedGrid
          items={filteredGroups}
          scrollElementRef={scrollWrapperRef}
          minItemWidth={160}
          estimateRowHeight={230}
          gap={12}
          className="p-4"
          getItemKey={(g) => g.key}
          renderItem={(g) => <AlbumCard group={g} onOpen={openAlbum} />}
        />
      </div>
    </div>
  );
}
