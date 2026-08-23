import { ChevronLeft, ChevronRight } from "lucide-react";
import { memo, useCallback, useDeferredValue, useMemo, useRef, useState } from "react";
import { type ArtistGroup, groupArtists } from "../lib/trackGroups";
import { useLibraryStore } from "../store/libraryStore";
import { usePlayerStore } from "../store/playerStore";
import { useQueueStore } from "../store/queueStore";
import { useSearchStore } from "../store/searchStore";
import { TrackCover } from "./TrackCover";
import { VirtualizedList } from "./VirtualizedList";

function ArtistDetail({ group, onBack }: { group: ArtistGroup; onBack: () => void }) {
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
          Все исполнители
        </button>

        <h1 className="mb-4 text-xl font-semibold text-text-primary">{group.artist}</h1>
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
                className={`flex items-center justify-between gap-3 rounded-md px-3 py-2 text-left ${
                  currentPath === t.path ? "bg-card-hover" : "hover:bg-card-hover"
                }`}
              >
                <span className="truncate text-sm text-text-primary">{t.title}</span>
                <span className="flex-shrink-0 truncate text-xs text-text-secondary">
                  {t.album ?? ""}
                </span>
              </button>
            )}
          />
        </div>
      </div>
    </div>
  );
}

const ArtistRow = memo(function ArtistRow({
  group,
  onOpen,
}: {
  group: ArtistGroup;
  onOpen: (artist: string) => void;
}) {
  return (
    <button
      onClick={() => onOpen(group.artist)}
      className="flex w-full items-center gap-3 rounded-md px-3 py-2 text-left transition-[background-color,transform] duration-150 hover:translate-x-0.5 hover:bg-card-hover"
    >
      <TrackCover
        path={group.tracks[0]?.path}
        className="flex h-10 w-10 flex-shrink-0 items-center justify-center overflow-hidden rounded-full bg-card-background shadow-sm"
      />
      <div className="min-w-0 flex-1">
        <div className="truncate text-sm text-text-primary">{group.artist}</div>
        <div className="truncate text-xs text-text-secondary">{group.tracks.length} треков</div>
      </div>
      <ChevronRight size={16} className="text-text-secondary" />
    </button>
  );
});

export function ArtistsView() {
  const tracks = useLibraryStore((s) => s.tracks);
  const query = useSearchStore((s) => s.query);
  const [selectedArtist, setSelectedArtist] = useState<string | null>(null);
  const scrollWrapperRef = useRef<HTMLDivElement>(null);
  const openArtist = useCallback((artist: string) => setSelectedArtist(artist), []);

  const groups = useMemo(() => groupArtists(tracks), [tracks]);
  const selected = groups.find((g) => g.artist === selectedArtist) ?? null;

  const deferredQuery = useDeferredValue(query);
  const filteredGroups = useMemo(() => {
    const q = deferredQuery.trim().toLowerCase();
    if (!q) return groups;
    return groups.filter((g) => g.artist.toLowerCase().includes(q));
  }, [groups, deferredQuery]);

  if (tracks.length === 0) {
    return (
      <div className="flex h-full items-center justify-center text-text-secondary">
        Библиотека пуста — добавьте папку с музыкой
      </div>
    );
  }

  if (selected) {
    return <ArtistDetail group={selected} onBack={() => setSelectedArtist(null)} />;
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
        <VirtualizedList
          items={filteredGroups}
          scrollElementRef={scrollWrapperRef}
          estimateSize={52}
          gap={4}
          className="p-4"
          getItemKey={(g) => g.artist}
          renderItem={(g) => <ArtistRow group={g} onOpen={openArtist} />}
        />
      </div>
    </div>
  );
}
