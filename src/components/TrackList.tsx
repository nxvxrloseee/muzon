import { useVirtualizer } from "@tanstack/react-virtual";
import { AnimatePresence, motion } from "motion/react";
import {
  Check,
  Heart,
  LayoutGrid,
  List,
  ListPlus,
  Music2,
  Pencil,
  X,
} from "lucide-react";
import { type RefObject, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useLenis } from "../hooks/useLenis";
import { useLibraryViewMode } from "../hooks/useLibraryViewMode";
import { useTrackCover } from "../hooks/useTrackCover";
import { trackMatchesQuery } from "../lib/trackSearch";
import { useLibraryStore } from "../store/libraryStore";
import { usePlayerStore } from "../store/playerStore";
import { usePlaylistStore } from "../store/playlistStore";
import { useQueueStore } from "../store/queueStore";
import { useSearchStore } from "../store/searchStore";
import type { Track } from "../types";
import { TagEditor } from "./TagEditor";
import { VirtualizedList } from "./VirtualizedList";

interface TrackItemProps {
  track: Track;
  allTracks: Track[];
  onEdit: () => void;
  selectionActive: boolean;
  selected: boolean;
  onToggleSelect: () => void;
}

function TrackActions({ track, onEdit }: { track: Track; onEdit: () => void }) {
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
              title="Добавить в плейлист"
            />
          }
        >
          <ListPlus size={13} />
        </DropdownMenuTrigger>
        <PlaylistDropdownItems trackIds={[track.id]} />
      </DropdownMenu>

      <button
        onClick={(e) => {
          e.stopPropagation();
          onEdit();
        }}
        className="flex h-7 w-7 items-center justify-center rounded-full bg-black/60 text-white hover:bg-black/80"
        title="Редактировать тег"
      >
        <Pencil size={13} />
      </button>
    </div>
  );
}

function TrackCard({ track, allTracks, onEdit, selectionActive, selected, onToggleSelect }: TrackItemProps) {
  const cover = useTrackCover(track.path);
  const setQueue = useQueueStore((s) => s.setQueue);
  const currentPath = usePlayerStore((s) => s.currentPath);
  const isActive = currentPath === track.path;

  return (
    <motion.div
      initial={{ opacity: 0, scale: 0.95 }}
      animate={{ opacity: 1, scale: 1 }}
      whileHover={{ scale: 1.02 }}
      transition={{ type: "spring", stiffness: 350, damping: 28 }}
      className={`group relative flex flex-col gap-2 rounded-lg p-3 transition-colors ${
        isActive ? "bg-card-hover" : "hover:bg-card-hover"
      }`}
    >
      <button
        onClick={(e) => {
          e.stopPropagation();
          onToggleSelect();
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
        onClick={() => (selectionActive ? onToggleSelect() : setQueue(allTracks, track))}
        className="flex flex-col gap-2 text-left"
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
          <div className="truncate text-sm font-medium text-text-primary">{track.title}</div>
          <div className="truncate text-xs text-text-secondary">
            {track.artist ?? "Неизвестный исполнитель"}
          </div>
        </div>
      </button>
    </motion.div>
  );
}

function formatDuration(secs: number | null): string {
  if (secs == null) return "--:--";
  const m = Math.floor(secs / 60);
  const s = Math.floor(secs % 60);
  return `${m}:${s.toString().padStart(2, "0")}`;
}

function TrackRow({ track, allTracks, onEdit, selectionActive, selected, onToggleSelect }: TrackItemProps) {
  const cover = useTrackCover(track.path);
  const setQueue = useQueueStore((s) => s.setQueue);
  const currentPath = usePlayerStore((s) => s.currentPath);
  const isActive = currentPath === track.path;

  return (
    <motion.div
      initial={{ opacity: 0, y: -4 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ type: "spring", stiffness: 400, damping: 32 }}
      className={`group flex items-center gap-3 rounded-md px-3 py-2 ${
        isActive ? "bg-card-hover" : "hover:bg-card-hover"
      }`}
    >
      <button
        onClick={() => onToggleSelect()}
        className={`flex h-5 w-5 flex-shrink-0 items-center justify-center rounded-full border transition-opacity ${
          selected
            ? "border-accent-primary bg-accent-primary text-white opacity-100"
            : "border-divider text-transparent opacity-0 group-hover:opacity-100"
        } ${selectionActive ? "opacity-100" : ""}`}
      >
        <Check size={11} />
      </button>

      <button
        onClick={() => (selectionActive ? onToggleSelect() : setQueue(allTracks, track))}
        className="flex flex-1 items-center gap-3 overflow-hidden text-left"
      >
        <div className="flex h-10 w-10 flex-shrink-0 items-center justify-center overflow-hidden rounded bg-card-background">
          {cover ? (
            <img src={cover} alt="" className="h-full w-full object-cover" />
          ) : (
            <Music2 size={16} className="text-text-secondary/40" />
          )}
        </div>
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
    </motion.div>
  );
}

function PlaylistDropdownItems({ trackIds }: { trackIds: number[] }) {
  const playlists = usePlaylistStore((s) => s.playlists);
  const addTrack = usePlaylistStore((s) => s.addTrack);
  const setFavorite = useLibraryStore((s) => s.setFavorite);

  return (
    <DropdownMenuContent align="end">
      <DropdownMenuItem onClick={() => trackIds.forEach((id) => setFavorite(id, true))}>
        <Heart size={13} className="mr-2 text-red-500" fill="currentColor" />
        Любимые
      </DropdownMenuItem>
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

interface VirtualListProps {
  tracks: Track[];
  allTracks: Track[];
  scrollElementRef: RefObject<HTMLDivElement | null>;
  onEdit: (track: Track) => void;
  selected: Set<number>;
  selectionActive: boolean;
  onToggleSelect: (id: number) => void;
}

function VirtualGrid({
  tracks,
  allTracks,
  scrollElementRef,
  onEdit,
  selected,
  selectionActive,
  onToggleSelect,
}: VirtualListProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const width = useElementWidth(containerRef);
  const columns = width > 0 ? Math.max(1, Math.floor((width + GRID_GAP) / (GRID_MIN_ITEM + GRID_GAP))) : 0;

  const rows = useMemo(() => {
    if (columns === 0) return [];
    const chunked: Track[][] = [];
    for (let i = 0; i < tracks.length; i += columns) {
      chunked.push(tracks.slice(i, i + columns));
    }
    return chunked;
  }, [tracks, columns]);

  const rowVirtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollElementRef.current,
    estimateSize: () => 230,
    overscan: 4,
  });

  return (
    <div ref={containerRef} className="px-4 pt-4 pb-4">
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
                  paddingBottom: virtualRow.index < rows.length - 1 ? GRID_GAP : 0,
                }}
              >
                <div
                  className="grid gap-3"
                  style={{ gridTemplateColumns: `repeat(${columns}, minmax(${GRID_MIN_ITEM}px, 1fr))` }}
                >
                  {row.map((t) => (
                    <TrackCard
                      key={t.id}
                      track={t}
                      allTracks={allTracks}
                      onEdit={() => onEdit(t)}
                      selectionActive={selectionActive}
                      selected={selected.has(t.id)}
                      onToggleSelect={() => onToggleSelect(t.id)}
                    />
                  ))}
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

function VirtualList({
  tracks,
  allTracks,
  scrollElementRef,
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
          allTracks={allTracks}
          onEdit={() => onEdit(t)}
          selectionActive={selectionActive}
          selected={selected.has(t.id)}
          onToggleSelect={() => onToggleSelect(t.id)}
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
  const [editingTrack, setEditingTrack] = useState<Track | null>(null);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const scrollWrapperRef = useRef<HTMLDivElement>(null);
  const scrollContentRef = useRef<HTMLDivElement>(null);
  useLenis(scrollWrapperRef, scrollContentRef);

  const tracks = useMemo(
    () => allTracks.filter((t) => trackMatchesQuery(t, query)),
    [allTracks, query],
  );

  useEffect(() => {
    refreshPlaylists();
  }, [refreshPlaylists]);

  function toggleSelect(id: number) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

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

      <div ref={scrollWrapperRef} className="flex-1 overflow-y-auto">
        <div ref={scrollContentRef}>
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
                  allTracks={tracks}
                  scrollElementRef={scrollWrapperRef}
                  onEdit={setEditingTrack}
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
                  allTracks={tracks}
                  scrollElementRef={scrollWrapperRef}
                  onEdit={setEditingTrack}
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
              Добавить в плейлист
            </DropdownMenuTrigger>
            <PlaylistDropdownItems trackIds={[...selected]} />
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
