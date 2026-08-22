import { ChevronLeft, Music2 } from "lucide-react";
import { motion } from "motion/react";
import { useMemo, useState } from "react";
import { useTrackCover } from "../hooks/useTrackCover";
import { type AlbumGroup, groupAlbums } from "../lib/trackGroups";
import { useLibraryStore } from "../store/libraryStore";
import { usePlayerStore } from "../store/playerStore";
import { useQueueStore } from "../store/queueStore";
import { useSearchStore } from "../store/searchStore";

function AlbumCard({ group, onOpen }: { group: AlbumGroup; onOpen: () => void }) {
  const cover = useTrackCover(group.tracks[0]?.path);

  return (
    <motion.button
      onClick={onOpen}
      whileHover={{ scale: 1.02 }}
      whileTap={{ scale: 0.98 }}
      transition={{ type: "spring", stiffness: 350, damping: 28 }}
      className="flex flex-col gap-2 rounded-lg p-3 text-left transition-colors hover:bg-card-hover"
    >
      <div className="aspect-square w-full overflow-hidden rounded-md bg-card-background shadow-md">
        {cover ? (
          <img src={cover} alt="" className="h-full w-full object-cover" />
        ) : (
          <div className="flex h-full w-full items-center justify-center text-text-secondary/40">
            <Music2 size={28} />
          </div>
        )}
      </div>
      <div className="min-w-0">
        <div className="truncate text-sm font-medium text-text-primary">{group.album}</div>
        <div className="truncate text-xs text-text-secondary">{group.artist}</div>
      </div>
    </motion.button>
  );
}

function AlbumDetail({ group, onBack }: { group: AlbumGroup; onBack: () => void }) {
  const cover = useTrackCover(group.tracks[0]?.path);
  const setQueue = useQueueStore((s) => s.setQueue);
  const currentPath = usePlayerStore((s) => s.currentPath);

  return (
    <div className="p-4">
      <button
        onClick={onBack}
        className="mb-4 flex items-center gap-1 text-sm text-text-secondary hover:text-text-primary"
      >
        <ChevronLeft size={16} />
        Все альбомы
      </button>

      <div className="mb-6 flex items-end gap-4">
        <div className="flex h-32 w-32 flex-shrink-0 items-center justify-center overflow-hidden rounded-md bg-card-background">
          {cover ? (
            <img src={cover} alt="" className="h-full w-full object-cover" />
          ) : (
            <Music2 size={32} className="text-text-secondary/40" />
          )}
        </div>
        <div>
          <div className="text-xl font-semibold text-text-primary">{group.album}</div>
          <div className="text-sm text-text-secondary">{group.artist}</div>
        </div>
      </div>

      <div className="flex flex-col gap-1">
        {group.tracks.map((t) => (
          <button
            key={t.id}
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
        ))}
      </div>
    </div>
  );
}

export function AlbumsView() {
  const tracks = useLibraryStore((s) => s.tracks);
  const query = useSearchStore((s) => s.query);
  const [selectedKey, setSelectedKey] = useState<string | null>(null);

  const groups = useMemo(() => groupAlbums(tracks), [tracks]);
  const selected = groups.find((g) => g.key === selectedKey) ?? null;

  const filteredGroups = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return groups;
    return groups.filter(
      (g) => g.album.toLowerCase().includes(q) || g.artist.toLowerCase().includes(q),
    );
  }, [groups, query]);

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
    <div className="grid grid-cols-[repeat(auto-fill,minmax(160px,1fr))] gap-3 p-4">
      {filteredGroups.map((g) => (
        <AlbumCard key={g.key} group={g} onOpen={() => setSelectedKey(g.key)} />
      ))}
    </div>
  );
}
