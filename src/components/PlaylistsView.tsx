import { Reorder } from "motion/react";
import { Heart, Music2, Pencil, Plus, Trash2, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useLenis } from "../hooks/useLenis";
import { useTrackCover } from "../hooks/useTrackCover";
import { useLibraryStore } from "../store/libraryStore";
import { usePlaylistStore } from "../store/playlistStore";
import { useQueueStore } from "../store/queueStore";
import { useSearchStore } from "../store/searchStore";
import type { Track } from "../types";
import { VirtualizedList } from "./VirtualizedList";

function TrackRow({
  track,
  onPlay,
  onRemove,
  removeIcon,
  removeTitle,
  draggable,
}: {
  track: Track;
  onPlay: () => void;
  onRemove: () => void;
  removeIcon: React.ReactNode;
  removeTitle: string;
  draggable: boolean;
}) {
  const cover = useTrackCover(track.path);

  const content = (
    <>
      <button onClick={onPlay} className="flex flex-1 items-center gap-3 text-left">
        <div className="flex h-10 w-10 flex-shrink-0 items-center justify-center overflow-hidden rounded bg-card-hover">
          {cover ? (
            <img src={cover} alt="" className="h-full w-full object-cover" />
          ) : (
            <Music2 size={16} className="text-text-secondary/40" />
          )}
        </div>
        <div className="min-w-0">
          <div className="truncate text-sm text-text-primary">{track.title}</div>
          <div className="truncate text-xs text-text-secondary">
            {track.artist ?? "Неизвестный исполнитель"}
          </div>
        </div>
      </button>
      <button
        onClick={onRemove}
        className="flex h-7 w-7 flex-shrink-0 items-center justify-center rounded-full text-text-secondary hover:bg-card-hover hover:text-red-400"
        title={removeTitle}
      >
        {removeIcon}
      </button>
    </>
  );

  if (draggable) {
    return (
      <Reorder.Item
        value={track}
        className="flex items-center gap-3 rounded-md bg-card-background px-3 py-2"
      >
        {content}
      </Reorder.Item>
    );
  }
  return (
    <div className="flex items-center gap-3 rounded-md bg-card-background px-3 py-2">
      {content}
    </div>
  );
}

export function PlaylistsView() {
  const {
    playlists,
    selectedId,
    tracks,
    refreshPlaylists,
    selectPlaylist,
    createPlaylist,
    renamePlaylist,
    deletePlaylist,
    removeTrack,
    reorderTracks,
  } = usePlaylistStore();
  const setQueue = useQueueStore((s) => s.setQueue);
  const libraryTracks = useLibraryStore((s) => s.tracks);
  const setFavorite = useLibraryStore((s) => s.setFavorite);
  const query = useSearchStore((s) => s.query);

  const [showFavorites, setShowFavorites] = useState(false);
  const [newName, setNewName] = useState("");
  const [renamingId, setRenamingId] = useState<number | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const scrollWrapperRef = useRef<HTMLDivElement>(null);
  const scrollContentRef = useRef<HTMLDivElement>(null);
  useLenis(scrollWrapperRef, scrollContentRef);

  useEffect(() => {
    refreshPlaylists();
  }, [refreshPlaylists]);

  const favoriteTracks = useMemo(
    () => libraryTracks.filter((t) => t.is_favorite),
    [libraryTracks],
  );

  const selected = playlists.find((p) => p.id === selectedId) ?? null;

  const filteredPlaylists = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return playlists;
    return playlists.filter((p) => p.name.toLowerCase().includes(q));
  }, [playlists, query]);

  async function handleCreate() {
    if (!newName.trim()) return;
    await createPlaylist(newName.trim());
    setNewName("");
  }

  function openPlaylist(id: number) {
    setShowFavorites(false);
    selectPlaylist(id);
  }

  function openFavorites() {
    setShowFavorites(true);
    selectPlaylist(null);
  }

  return (
    <div className="flex h-full">
      <div className="flex w-64 flex-shrink-0 flex-col gap-2 border-r border-divider p-4">
        <div className="flex gap-2">
          <Input
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleCreate()}
            placeholder="Новый плейлист"
          />
          <Button size="icon" onClick={handleCreate}>
            <Plus size={16} />
          </Button>
        </div>

        <div className="mt-2 flex flex-col gap-1">
          <button
            onClick={openFavorites}
            className={`flex items-center gap-2 rounded-md px-3 py-2 text-left text-sm ${
              showFavorites ? "bg-card-hover text-text-primary" : "text-text-primary hover:bg-card-hover"
            }`}
          >
            <Heart size={14} className="text-red-500" fill="currentColor" />
            Любимые{" "}
            <span className="text-xs text-text-secondary">({favoriteTracks.length})</span>
          </button>

          {filteredPlaylists.map((p) => (
            <div
              key={p.id}
              className={`group flex items-center gap-2 rounded-md px-3 py-2 ${
                !showFavorites && p.id === selectedId ? "bg-card-hover" : "hover:bg-card-hover"
              }`}
            >
              {renamingId === p.id ? (
                <Input
                  autoFocus
                  value={renameValue}
                  onChange={(e) => setRenameValue(e.target.value)}
                  onBlur={() => {
                    if (renameValue.trim()) renamePlaylist(p.id, renameValue.trim());
                    setRenamingId(null);
                  }}
                  onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
                  className="h-7"
                />
              ) : (
                <button
                  onClick={() => openPlaylist(p.id)}
                  className="flex-1 truncate text-left text-sm text-text-primary"
                >
                  {p.name}{" "}
                  <span className="text-xs text-text-secondary">({p.track_count})</span>
                </button>
              )}
              <button
                onClick={() => {
                  setRenamingId(p.id);
                  setRenameValue(p.name);
                }}
                className="flex h-6 w-6 flex-shrink-0 items-center justify-center rounded text-text-secondary opacity-0 hover:text-text-primary group-hover:opacity-100"
              >
                <Pencil size={12} />
              </button>
              <button
                onClick={() => deletePlaylist(p.id)}
                className="flex h-6 w-6 flex-shrink-0 items-center justify-center rounded text-text-secondary opacity-0 hover:text-red-400 group-hover:opacity-100"
              >
                <Trash2 size={12} />
              </button>
            </div>
          ))}
        </div>
      </div>

      <div ref={scrollWrapperRef} className="flex-1 overflow-y-auto p-4">
        <div ref={scrollContentRef}>
        {showFavorites ? (
          favoriteTracks.length === 0 ? (
            <div className="flex h-full items-center justify-center text-text-secondary">
              Пока нет любимых треков — отметьте их сердечком в библиотеке
            </div>
          ) : (
            // Padding already lives on the scroll wrapper below (p-4), so no
            // className/gap padding is needed here.
            <VirtualizedList
              items={favoriteTracks}
              scrollElementRef={scrollWrapperRef}
              estimateSize={56}
              gap={4}
              overscan={8}
              getItemKey={(t) => t.id}
              renderItem={(t) => (
                <TrackRow
                  track={t}
                  draggable={false}
                  onPlay={() => setQueue(favoriteTracks, t)}
                  onRemove={() => setFavorite(t.id, false)}
                  removeIcon={<Heart size={14} fill="currentColor" />}
                  removeTitle="Убрать из любимых"
                />
              )}
            />
          )
        ) : !selected ? (
          <div className="flex h-full items-center justify-center text-text-secondary">
            Выберите плейлист слева
          </div>
        ) : tracks.length === 0 ? (
          <div className="flex h-full items-center justify-center text-text-secondary">
            Плейлист «{selected.name}» пуст — добавьте треки из библиотеки
          </div>
        ) : (
          // Intentionally not virtualized: Reorder.Group measures every sibling
          // Reorder.Item to compute drag targets, so windowing this list would
          // silently break dragging to any position outside the current
          // viewport. A single playlist is bounded by how many tracks a user
          // adds by hand, unlike the full library or "Избранное" (already
          // virtualized above), so this is an acceptable tradeoff.
          <Reorder.Group
            axis="y"
            values={tracks}
            onReorder={(newOrder) => reorderTracks(selected.id, newOrder.map((t) => t.id))}
            className="flex flex-col gap-1"
          >
            {tracks.map((t) => (
              <TrackRow
                key={t.id}
                track={t}
                draggable
                onPlay={() => setQueue(tracks, t)}
                onRemove={() => removeTrack(selected.id, t.id)}
                removeIcon={<X size={14} />}
                removeTitle="Убрать из плейлиста"
              />
            ))}
          </Reorder.Group>
        )}
        </div>
      </div>
    </div>
  );
}
