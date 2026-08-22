import { ChevronLeft, ChevronRight, Music2 } from "lucide-react";
import { motion } from "motion/react";
import { useMemo, useRef, useState } from "react";
import { useLenis } from "../hooks/useLenis";
import { useTrackCover } from "../hooks/useTrackCover";
import { type ArtistGroup, groupArtists } from "../lib/trackGroups";
import { useLibraryStore } from "../store/libraryStore";
import { usePlayerStore } from "../store/playerStore";
import { useQueueStore } from "../store/queueStore";
import { useSearchStore } from "../store/searchStore";
import { VirtualizedList } from "./VirtualizedList";

function ArtistDetail({ group, onBack }: { group: ArtistGroup; onBack: () => void }) {
  const setQueue = useQueueStore((s) => s.setQueue);
  const currentPath = usePlayerStore((s) => s.currentPath);
  const scrollWrapperRef = useRef<HTMLDivElement>(null);
  const scrollContentRef = useRef<HTMLDivElement>(null);
  useLenis(scrollWrapperRef, scrollContentRef);

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

      <div ref={scrollWrapperRef} className="flex-1 overflow-y-auto">
        <div ref={scrollContentRef}>
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

function ArtistRow({ group, onOpen }: { group: ArtistGroup; onOpen: () => void }) {
  const cover = useTrackCover(group.tracks[0]?.path);

  return (
    <motion.button
      onClick={onOpen}
      whileHover={{ x: 2 }}
      whileTap={{ scale: 0.98 }}
      transition={{ type: "spring", stiffness: 400, damping: 32 }}
      className="flex items-center gap-3 rounded-md px-3 py-2 text-left hover:bg-card-hover"
    >
      <div className="flex h-10 w-10 flex-shrink-0 items-center justify-center overflow-hidden rounded-full bg-card-background shadow-sm">
        {cover ? (
          <img src={cover} alt="" className="h-full w-full object-cover" />
        ) : (
          <Music2 size={16} className="text-text-secondary/40" />
        )}
      </div>
      <div className="min-w-0 flex-1">
        <div className="truncate text-sm text-text-primary">{group.artist}</div>
        <div className="truncate text-xs text-text-secondary">{group.tracks.length} треков</div>
      </div>
      <ChevronRight size={16} className="text-text-secondary" />
    </motion.button>
  );
}

export function ArtistsView() {
  const tracks = useLibraryStore((s) => s.tracks);
  const query = useSearchStore((s) => s.query);
  const [selectedArtist, setSelectedArtist] = useState<string | null>(null);

  const groups = useMemo(() => groupArtists(tracks), [tracks]);
  const selected = groups.find((g) => g.artist === selectedArtist) ?? null;

  const filteredGroups = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return groups;
    return groups.filter((g) => g.artist.toLowerCase().includes(q));
  }, [groups, query]);

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
    <div className="flex flex-col gap-1 p-4">
      {filteredGroups.map((g) => (
        <ArtistRow key={g.artist} group={g} onOpen={() => setSelectedArtist(g.artist)} />
      ))}
    </div>
  );
}
