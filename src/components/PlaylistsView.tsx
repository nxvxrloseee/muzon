import { Reorder } from "motion/react";
import {
  CalendarPlus,
  Flame,
  Heart,
  History,
  Hourglass,
  type LucideIcon,
  Pencil,
  Plus,
  Save,
  Sparkles,
  Trash2,
  X,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { tracksLabel } from "../lib/plural";
import { SMART_PLAYLISTS, type SmartPlaylistId, smartPlaylist } from "../lib/smartPlaylists";
import { useLibraryStore } from "../store/libraryStore";
import { usePlaylistStore } from "../store/playlistStore";
import { useQueueStore } from "../store/queueStore";
import { useSearchStore } from "../store/searchStore";
import type { Track } from "../types";
import { TrackCover } from "./TrackCover";
import { VirtualizedList } from "./VirtualizedList";

const SMART_ICONS: Record<SmartPlaylistId, LucideIcon> = {
  favorites: Heart,
  "most-played": Flame,
  "recently-played": History,
  forgotten: Hourglass,
  "never-played": Sparkles,
  "recently-added": CalendarPlus,
};

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
  /** Smart lists other than favourites have nothing to remove a track from. */
  onRemove?: () => void;
  removeIcon?: React.ReactNode;
  removeTitle?: string;
  draggable: boolean;
}) {
  const content = (
    <>
      <button onClick={onPlay} className="flex flex-1 items-center gap-3 text-left">
        <TrackCover
          path={track.path}
          className="flex h-10 w-10 flex-shrink-0 items-center justify-center overflow-hidden rounded bg-card-hover"
        />
        <div className="min-w-0">
          <div className="truncate text-sm text-text-primary">{track.title}</div>
          <div className="truncate text-xs text-text-secondary">
            {track.artist ?? "Неизвестный исполнитель"}
          </div>
        </div>
      </button>
      {onRemove && (
        <button
          onClick={onRemove}
          className="flex h-7 w-7 flex-shrink-0 items-center justify-center rounded-full text-text-secondary hover:bg-card-hover hover:text-red-400"
          title={removeTitle}
        >
          {removeIcon}
        </button>
      )}
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
  const playlists = usePlaylistStore((s) => s.playlists);
  const selectedId = usePlaylistStore((s) => s.selectedId);
  const tracks = usePlaylistStore((s) => s.tracks);
  const refreshPlaylists = usePlaylistStore((s) => s.refreshPlaylists);
  const selectPlaylist = usePlaylistStore((s) => s.selectPlaylist);
  const createPlaylist = usePlaylistStore((s) => s.createPlaylist);
  const renamePlaylist = usePlaylistStore((s) => s.renamePlaylist);
  const deletePlaylist = usePlaylistStore((s) => s.deletePlaylist);
  const removeTrack = usePlaylistStore((s) => s.removeTrack);
  const reorderTracks = usePlaylistStore((s) => s.reorderTracks);
  const saveAsPlaylist = usePlaylistStore((s) => s.saveAsPlaylist);
  const setQueue = useQueueStore((s) => s.setQueue);
  const libraryTracks = useLibraryStore((s) => s.tracks);
  const setFavorite = useLibraryStore((s) => s.setFavorite);
  const query = useSearchStore((s) => s.query);

  const [smartId, setSmartId] = useState<SmartPlaylistId | null>(null);
  const [newName, setNewName] = useState("");
  const [renamingId, setRenamingId] = useState<number | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const scrollWrapperRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    refreshPlaylists();
  }, [refreshPlaylists]);

  // Every smart list at once, for the counts beside their names. The library
  // only changes on a scan, a like or a counted listen, so this is cheap.
  const smartTracks = useMemo(() => {
    const now = Date.now() / 1000;
    return new Map(SMART_PLAYLISTS.map((p) => [p.id, p.select(libraryTracks, now)]));
  }, [libraryTracks]);
  const shownSmart = smartId ? smartPlaylist(smartId) : null;
  const shownSmartTracks = smartId ? smartTracks.get(smartId)! : [];

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
    setSmartId(null);
    selectPlaylist(id);
  }

  function openSmart(id: SmartPlaylistId) {
    setSmartId(id);
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
          {SMART_PLAYLISTS.map((p) => {
            const Icon = SMART_ICONS[p.id];
            return (
              <button
                key={p.id}
                onClick={() => openSmart(p.id)}
                className={`flex items-center gap-2 rounded-md px-3 py-2 text-left text-sm text-text-primary ${
                  smartId === p.id ? "bg-card-hover" : "hover:bg-card-hover"
                }`}
              >
                {p.id === "favorites" ? (
                  <Heart size={14} className="text-red-500" fill="currentColor" />
                ) : (
                  <Icon size={14} className="text-accent-primary" />
                )}
                <span className="truncate">{p.name}</span>
                <span className="text-xs text-text-secondary">({smartTracks.get(p.id)!.length})</span>
              </button>
            );
          })}

          <div className="my-1 border-t border-divider" />

          {filteredPlaylists.map((p) => (
            <div
              key={p.id}
              className={`group flex items-center gap-2 rounded-md px-3 py-2 ${
                !smartId && p.id === selectedId ? "bg-card-hover" : "hover:bg-card-hover"
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

      <div ref={scrollWrapperRef} data-lenis-prevent className="flex-1 overflow-y-auto p-4">
        <div>
        {shownSmart ? (
          shownSmartTracks.length === 0 ? (
            <div className="flex h-full items-center justify-center px-8 text-center text-text-secondary">
              {shownSmart.empty}
            </div>
          ) : (
            <>
              <div className="mb-3 flex items-center justify-between gap-3">
                <div>
                  <h1 className="text-lg font-semibold text-text-primary">{shownSmart.name}</h1>
                  <div className="text-xs text-text-secondary">
                    {tracksLabel(shownSmartTracks.length)} · обновляется сам
                  </div>
                </div>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() =>
                    saveAsPlaylist(
                      `${shownSmart.name} · ${new Date().toLocaleDateString("ru-RU")}`,
                      shownSmartTracks.map((t) => t.id),
                    )
                  }
                  title="Сохранить нынешний состав как обычный плейлист"
                >
                  <Save size={14} />
                  Сохранить как плейлист
                </Button>
              </div>
              {/* Padding already lives on the scroll wrapper below (p-4), so no
                  className/gap padding is needed here. */}
              <VirtualizedList
                items={shownSmartTracks}
                scrollElementRef={scrollWrapperRef}
                estimateSize={56}
                gap={4}
                overscan={8}
                getItemKey={(t) => t.id}
                renderItem={(t) =>
                  smartId === "favorites" ? (
                    <TrackRow
                      track={t}
                      draggable={false}
                      onPlay={() => setQueue(shownSmartTracks, t)}
                      onRemove={() => setFavorite(t.id, false)}
                      removeIcon={<Heart size={14} fill="currentColor" />}
                      removeTitle="Убрать из любимых"
                    />
                  ) : (
                    <TrackRow
                      track={t}
                      draggable={false}
                      onPlay={() => setQueue(shownSmartTracks, t)}
                    />
                  )
                }
              />
            </>
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
