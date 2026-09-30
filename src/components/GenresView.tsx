import { ChevronLeft, ChevronRight, Tags } from "lucide-react";
import { memo, useCallback, useDeferredValue, useMemo, useRef, useState } from "react";
import { type GenreGroup, groupGenres } from "../lib/trackGroups";
import { useLibraryStore } from "../store/libraryStore";
import { usePlayerStore } from "../store/playerStore";
import { useQueueStore } from "../store/queueStore";
import { useSearchStore } from "../store/searchStore";
import { useStableGroups } from "../hooks/useStableGroups";
import { tracksLabel } from "../lib/plural";
import { VirtualizedList } from "./VirtualizedList";

function GenreDetail({ group, onBack }: { group: GenreGroup; onBack: () => void }) {
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
          Все жанры
        </button>

        <h1 className="mb-4 text-xl font-semibold text-text-primary">{group.genre}</h1>
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
                  {t.artist ?? ""}
                </span>
              </button>
            )}
          />
        </div>
      </div>
    </div>
  );
}

const GenreRow = memo(function GenreRow({
  group,
  onOpen,
}: {
  group: GenreGroup;
  onOpen: (genre: string) => void;
}) {
  return (
    <button
      onClick={() => onOpen(group.genre)}
      className="flex w-full items-center gap-3 rounded-md px-3 py-2 text-left transition-[background-color,transform] duration-150 hover:translate-x-0.5 hover:bg-card-hover"
    >
      <div className="flex h-10 w-10 flex-shrink-0 items-center justify-center rounded-full bg-card-background text-text-secondary shadow-sm">
        <Tags size={16} />
      </div>
      <div className="min-w-0 flex-1">
        <div className="truncate text-sm text-text-primary">{group.genre}</div>
        <div className="truncate text-xs text-text-secondary">{tracksLabel(group.tracks.length)}</div>
      </div>
      <ChevronRight size={16} className="text-text-secondary" />
    </button>
  );
});

const genreKey = (g: GenreGroup) => g.genre;

export function GenresView() {
  const tracks = useLibraryStore((s) => s.tracks);
  const query = useSearchStore((s) => s.query);
  const [selectedGenre, setSelectedGenre] = useState<string | null>(null);
  const scrollWrapperRef = useRef<HTMLDivElement>(null);
  const openGenre = useCallback((genre: string) => setSelectedGenre(genre), []);

  const groups = useStableGroups(tracks, groupGenres, genreKey);
  const selected = groups.find((g) => g.genre === selectedGenre) ?? null;

  const deferredQuery = useDeferredValue(query);
  const filteredGroups = useMemo(() => {
    const q = deferredQuery.trim().toLowerCase();
    if (!q) return groups;
    return groups.filter((g) => g.genre.toLowerCase().includes(q));
  }, [groups, deferredQuery]);

  if (tracks.length === 0) {
    return (
      <div className="flex h-full items-center justify-center text-text-secondary">
        Библиотека пуста — добавьте папку с музыкой
      </div>
    );
  }

  if (selected) {
    return <GenreDetail group={selected} onBack={() => setSelectedGenre(null)} />;
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
          getItemKey={genreKey}
          renderItem={(g) => <GenreRow group={g} onOpen={openGenre} />}
        />
      </div>
    </div>
  );
}
