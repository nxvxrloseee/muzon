import { AnimatePresence, motion } from "motion/react";
import {
  Check,
  CornerDownRight,
  Heart,
  LayoutGrid,
  List,
  ListPlus,
  ListFilter,
  Pencil,
  Plus,
  X,
} from "lucide-react";

import {
  memo,
  type RefObject,
  useCallback,
  useDeferredValue,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useLibrarySort } from "../hooks/useLibrarySort";
import { useLibraryViewMode } from "../hooks/useLibraryViewMode";
import { filterTracks } from "../lib/trackSearch";
import { LIBRARY_SORT_LABELS, sortTracks, type LibrarySort } from "../lib/trackSort";
import { useLibraryStore } from "../store/libraryStore";
import { usePlayerStore } from "../store/playerStore";
import { usePlaylistStore } from "../store/playlistStore";
import { useQueueStore } from "../store/queueStore";
import { useSearchStore } from "../store/searchStore";
import type { Track } from "../types";
import { TagEditor } from "./TagEditor";
import { TrackCover } from "./TrackCover";
import { VirtualizedGrid } from "./VirtualizedGrid";
import { VirtualizedList } from "./VirtualizedList";

/** Every callback here is stable across renders and the row passes its own
 * track/id back in, so `memo` can actually hold: re-rendering the list no
 * longer re-renders every visible row. */
interface TrackItemProps {
  track: Track;
  onPlay: (track: Track) => void;
  onEdit: (track: Track) => void;
  selectionActive: boolean;
  selected: boolean;
  onToggleSelect: (id: number) => void;
}

function TrackActions({ track, onEdit }: { track: Track; onEdit: (track: Track) => void }) {
  const toggleFavorite = useLibraryStore((s) => s.toggleFavorite);

  return (
    <div className="flex gap-1">
      <button
        onClick={(e) => {
          e.stopPropagation();
          toggleFavorite(track.id);
        }}
        className={`flex h-7 w-7 items-center justify-center rounded-full bg-black/60 transition-opacity hover:bg-black/80 ${
          track.is_favorite ? "text-red-500" : "text-white"
        }`}
        title="Любимое"
      >
        <Heart size={13} fill={track.is_favorite ? "currentColor" : "none"} />
      </button>

      <DropdownMenu>
        <DropdownMenuTrigger
          render={
            <button
              className="flex h-7 w-7 items-center justify-center rounded-full bg-black/60 text-white hover:bg-black/80"
              title="В очередь или плейлист"
            />
          }
        >
          <ListPlus size={13} />
        </DropdownMenuTrigger>
        <TrackActionItems trackIds={[track.id]} />
      </DropdownMenu>

      <button
        onClick={(e) => {
          e.stopPropagation();
          onEdit(track);
        }}
        className="flex h-7 w-7 items-center justify-center rounded-full bg-black/60 text-white hover:bg-black/80"
        title="Редактировать тег"
      >
        <Pencil size={13} />
      </button>
    </div>
  );
}

const TrackCard = memo(function TrackCard({
  track,
  onPlay,
  onEdit,
  selectionActive,
  selected,
  onToggleSelect,
}: TrackItemProps) {
  const currentPath = usePlayerStore((s) => s.currentPath);
  const isActive = currentPath === track.path;

  return (
    // Hover scaling as a CSS transition rather than a Motion spring: in a
    // windowed list every row that scrolls into view would otherwise mount an
    // animation, and the entrance spring ran on each of them mid-scroll.
    <div
      className={`group relative flex flex-col gap-2 rounded-lg p-3 transition-[background-color,transform] duration-150 hover:scale-[1.02] ${
        isActive ? "bg-card-hover" : "hover:bg-card-hover"
      }`}
    >
      <button
        onClick={(e) => {
          e.stopPropagation();
          onToggleSelect(track.id);
        }}
        className={`absolute left-4 top-4 z-10 flex h-6 w-6 items-center justify-center rounded-full border transition-opacity ${
          selected
            ? "border-accent-primary bg-accent-primary text-white opacity-100"
            : "border-white/60 bg-black/40 text-transparent opacity-0 group-hover:opacity-100"
        } ${selectionActive ? "opacity-100" : ""}`}
        title="Выбрать"
      >
        <Check size={12} />
      </button>

      <div className="absolute right-4 top-4 z-10 opacity-0 transition-opacity group-hover:opacity-100">
        <TrackActions track={track} onEdit={onEdit} />
      </div>

      <button
        onClick={() => (selectionActive ? onToggleSelect(track.id) : onPlay(track))}
        className="flex flex-col gap-2 text-left"
      >
        <TrackCover
          path={track.path}
          iconSize={28}
          className="flex aspect-square w-full items-center justify-center overflow-hidden rounded-md bg-card-background shadow-md"
        />
        <div className="min-w-0">
          <div className="truncate text-sm font-medium text-text-primary">{track.title}</div>
          <div className="truncate text-xs text-text-secondary">
            {track.artist ?? "Неизвестный исполнитель"}
          </div>
        </div>
      </button>
    </div>
  );
});

function formatDuration(secs: number | null): string {
  if (secs == null) return "--:--";
  const m = Math.floor(secs / 60);
  const s = Math.floor(secs % 60);
  return `${m}:${s.toString().padStart(2, "0")}`;
}

const TrackRow = memo(function TrackRow({
  track,
  onPlay,
  onEdit,
  selectionActive,
  selected,
  onToggleSelect,
}: TrackItemProps) {
  const currentPath = usePlayerStore((s) => s.currentPath);
  const isActive = currentPath === track.path;

  return (
    // No entrance animation: rows mount continuously while scrolling, so this
    // ran a spring per row per scroll rather than once per list.
    <div
      className={`group flex items-center gap-3 rounded-md px-3 py-2 transition-colors ${
        isActive ? "bg-card-hover" : "hover:bg-card-hover"
      }`}
    >
      <button
        onClick={() => onToggleSelect(track.id)}
        className={`flex h-5 w-5 flex-shrink-0 items-center justify-center rounded-full border transition-opacity ${
          selected
            ? "border-accent-primary bg-accent-primary text-white opacity-100"
            : "border-divider text-transparent opacity-0 group-hover:opacity-100"
        } ${selectionActive ? "opacity-100" : ""}`}
      >
        <Check size={11} />
      </button>

      <button
        onClick={() => (selectionActive ? onToggleSelect(track.id) : onPlay(track))}
        className="flex flex-1 items-center gap-3 overflow-hidden text-left"
      >
        <TrackCover
          path={track.path}
          className="flex h-10 w-10 flex-shrink-0 items-center justify-center overflow-hidden rounded bg-card-background"
        />
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm text-text-primary">{track.title}</div>
          <div className="truncate text-xs text-text-secondary">
            {track.artist ?? "Неизвестный исполнитель"}
          </div>
        </div>
        <div className="hidden w-40 flex-shrink-0 truncate text-xs text-text-secondary sm:block">
          {track.album ?? ""}
        </div>
        <div className="w-10 flex-shrink-0 text-right text-xs text-text-secondary">
          {formatDuration(track.duration_secs)}
        </div>
      </button>

      <div className="opacity-0 transition-opacity group-hover:opacity-100">
        <TrackActions track={track} onEdit={onEdit} />
      </div>
    </div>
  );
});

/** Resolves ids to tracks in library order, which is the order the user picked
 * them out of and so the order they should be queued in. */
function tracksByIds(trackIds: number[]): Track[] {
  const wanted = new Set(trackIds);
  return useLibraryStore.getState().tracks.filter((track) => wanted.has(track.id));
}

function TrackActionItems({ trackIds }: { trackIds: number[] }) {
  const playlists = usePlaylistStore((s) => s.playlists);
  const addTrack = usePlaylistStore((s) => s.addTrack);
  const setFavorite = useLibraryStore((s) => s.setFavorite);

  return (
    <DropdownMenuContent align="end">
      <DropdownMenuItem
        onClick={() => useQueueStore.getState().playNextInQueue(tracksByIds(trackIds))}
      >
        <CornerDownRight size={13} className="mr-2" />
        Играть следующим
      </DropdownMenuItem>
      <DropdownMenuItem onClick={() => useQueueStore.getState().enqueue(tracksByIds(trackIds))}>
        <Plus size={13} className="mr-2" />
        В конец очереди
      </DropdownMenuItem>

      <DropdownMenuSeparator />

      <DropdownMenuItem onClick={() => trackIds.forEach((id) => setFavorite(id, true))}>
        <Heart size={13} className="mr-2 text-red-500" fill="currentColor" />
        Любимые
      </DropdownMenuItem>
      <DropdownMenuLabel>Плейлисты</DropdownMenuLabel>
      {playlists.length === 0 ? (
        <DropdownMenuItem disabled>Нет плейлистов</DropdownMenuItem>
      ) : (
        playlists.map((p) => (
          <DropdownMenuItem key={p.id} onClick={() => trackIds.forEach((id) => addTrack(p.id, id))}>
            {p.name}
          </DropdownMenuItem>
        ))
      )}
    </DropdownMenuContent>
  );
}

const GRID_MIN_ITEM = 160;
const GRID_GAP = 12; // gap-3
const LIST_GAP = 4; // gap-1

interface VirtualListProps {
  tracks: Track[];
  scrollElementRef: RefObject<HTMLDivElement | null>;
  onPlay: (track: Track) => void;
  onEdit: (track: Track) => void;
  selected: Set<number>;
  selectionActive: boolean;
  onToggleSelect: (id: number) => void;
}

function VirtualGrid({
  tracks,
  scrollElementRef,
  onPlay,
  onEdit,
  selected,
  selectionActive,
  onToggleSelect,
}: VirtualListProps) {
  return (
    <VirtualizedGrid
      items={tracks}
      scrollElementRef={scrollElementRef}
      minItemWidth={GRID_MIN_ITEM}
      estimateRowHeight={230}
      gap={GRID_GAP}
      className="px-4 pt-4 pb-4"
      getItemKey={(t) => t.id}
      renderItem={(t) => (
        <TrackCard
          track={t}
          onPlay={onPlay}
          onEdit={onEdit}
          selectionActive={selectionActive}
          selected={selected.has(t.id)}
          onToggleSelect={onToggleSelect}
        />
      )}
    />
  );
}

function VirtualList({
  tracks,
  scrollElementRef,
  onPlay,
  onEdit,
  selected,
  selectionActive,
  onToggleSelect,
}: VirtualListProps) {
  return (
    <VirtualizedList
      items={tracks}
      scrollElementRef={scrollElementRef}
      estimateSize={52}
      gap={LIST_GAP}
      overscan={8}
      className="px-4 pt-4 pb-4"
      getItemKey={(t) => t.id}
      renderItem={(t) => (
        <TrackRow
          track={t}
          onPlay={onPlay}
          onEdit={onEdit}
          selectionActive={selectionActive}
          selected={selected.has(t.id)}
          onToggleSelect={onToggleSelect}
        />
      )}
    />
  );
}

export function TrackList() {
  const allTracks = useLibraryStore((s) => s.tracks);
  const refreshPlaylists = usePlaylistStore((s) => s.refreshPlaylists);
  const query = useSearchStore((s) => s.query);
  const [viewMode, setViewMode] = useLibraryViewMode();
  const [sort, setSort] = useLibrarySort();
  const [editingTrack, setEditingTrack] = useState<Track | null>(null);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const scrollWrapperRef = useRef<HTMLDivElement>(null);

  // Deferred so typing stays responsive: filtering a large library is the
  // expensive part of a keystroke, and React can keep the input painted while
  // the list catches up.
  const deferredQuery = useDeferredValue(query);
  const tracks = useMemo(
    () => sortTracks(filterTracks(allTracks, deferredQuery), sort),
    [allTracks, deferredQuery, sort],
  );

  // Read at click time by `handlePlay`, which stays a stable callback on
  // purpose - see below.
  const sortRef = useRef(sort);
  sortRef.current = sort;

  useEffect(() => {
    refreshPlaylists();
  }, [refreshPlaylists]);

  const toggleSelect = useCallback((id: number) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  // Reads the list to queue at click time instead of taking it as a prop, so
  // rows don't get a new prop identity every time the library array changes.
  // The queue has to come out in the order on screen, sort included, or playing
  // the top of "недавно добавленные" would queue the library alphabetically.
  const handlePlay = useCallback((track: Track) => {
    const visible = sortTracks(
      filterTracks(useLibraryStore.getState().tracks, useSearchStore.getState().query),
      sortRef.current,
    );
    useQueueStore.getState().setQueue(visible, track);
  }, []);

  const handleEdit = useCallback((track: Track) => setEditingTrack(track), []);

  if (allTracks.length === 0) {
    return (
      <div className="flex h-full items-center justify-center text-text-secondary">
        Библиотека пуста — добавьте папку с музыкой
      </div>
    );
  }

  return (
    <div className="relative flex h-full flex-col">
      <div className="flex items-center justify-between px-4 pt-3">
        <span className="text-xs text-text-secondary">
          {tracks.length} {tracks.length === 1 ? "трек" : "треков"}
        </span>
        <div className="flex items-center gap-2">
        <DropdownMenu>
          <DropdownMenuTrigger
            render={
              <button
                className="flex items-center gap-1.5 rounded-md bg-card-background px-2.5 py-1.5 text-xs text-text-secondary hover:bg-card-hover hover:text-text-primary"
                title="Порядок"
              />
            }
          >
            <ListFilter size={13} />
            {LIBRARY_SORT_LABELS[sort]}
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            {(Object.keys(LIBRARY_SORT_LABELS) as LibrarySort[]).map((option) => (
              <DropdownMenuItem key={option} onClick={() => setSort(option)}>
                {LIBRARY_SORT_LABELS[option]}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>

        <div className="flex gap-1 rounded-md bg-card-background p-0.5">
          <button
            onClick={() => setViewMode("grid")}
            className={`flex h-7 w-7 items-center justify-center rounded ${
              viewMode === "grid" ? "bg-card-hover text-text-primary" : "text-text-secondary"
            }`}
            title="Сетка"
          >
            <LayoutGrid size={14} />
          </button>
          <button
            onClick={() => setViewMode("list")}
            className={`flex h-7 w-7 items-center justify-center rounded ${
              viewMode === "list" ? "bg-card-hover text-text-primary" : "text-text-secondary"
            }`}
            title="Список"
          >
            <List size={14} />
          </button>
        </div>
        </div>
      </div>

      <div ref={scrollWrapperRef} data-lenis-prevent className="flex-1 overflow-y-auto">
        <div>
        {tracks.length === 0 ? (
          <div className="flex h-full items-center justify-center text-text-secondary">
            Ничего не найдено
          </div>
        ) : (
          <AnimatePresence mode="wait">
            {viewMode === "grid" ? (
              <motion.div
                key="grid"
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                exit={{ opacity: 0 }}
              >
                <VirtualGrid
                  tracks={tracks}
                  scrollElementRef={scrollWrapperRef}
                  onPlay={handlePlay}
                  onEdit={handleEdit}
                  selectionActive={selected.size > 0}
                  selected={selected}
                  onToggleSelect={toggleSelect}
                />
              </motion.div>
            ) : (
              <motion.div
                key="list"
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                exit={{ opacity: 0 }}
              >
                <VirtualList
                  tracks={tracks}
                  scrollElementRef={scrollWrapperRef}
                  onPlay={handlePlay}
                  onEdit={handleEdit}
                  selectionActive={selected.size > 0}
                  selected={selected}
                  onToggleSelect={toggleSelect}
                />
              </motion.div>
            )}
          </AnimatePresence>
        )}
        </div>
      </div>

      {selected.size > 0 && (
        <div className="flex items-center gap-3 border-t border-divider bg-card-background px-4 py-3">
          <span className="text-sm text-text-secondary">Выбрано: {selected.size}</span>
          <DropdownMenu>
            <DropdownMenuTrigger
              render={
                <button className="flex items-center gap-2 rounded-md bg-accent-primary/20 px-3 py-1.5 text-sm text-accent-primary" />
              }
            >
              <ListPlus size={14} />
              Действия
            </DropdownMenuTrigger>
            <TrackActionItems trackIds={[...selected]} />
          </DropdownMenu>
          <button
            onClick={() => setSelected(new Set())}
            className="ml-auto flex items-center gap-2 rounded-md px-3 py-1.5 text-sm text-text-secondary hover:bg-card-hover"
          >
            <X size={14} />
            Снять выделение
          </button>
        </div>
      )}

      <TagEditor
        track={editingTrack}
        open={editingTrack !== null}
        onOpenChange={(open) => !open && setEditingTrack(null)}
      />
    </div>
  );
}
