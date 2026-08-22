import { Command } from "cmdk";
import { Disc3, ListMusic, Mic2, Music2, Search } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { groupAlbums, groupArtists } from "../lib/trackGroups";
import { trackMatchesQuery } from "../lib/trackSearch";
import { useCommandPaletteStore } from "../store/commandPaletteStore";
import { useLibraryStore } from "../store/libraryStore";
import { usePlaylistStore } from "../store/playlistStore";
import { useQueueStore } from "../store/queueStore";
import { useSearchStore } from "../store/searchStore";
import { useUiStore } from "../store/uiStore";

const MAX_RESULTS_PER_GROUP = 8;

export function CommandPalette() {
  const open = useCommandPaletteStore((s) => s.open);
  const setOpen = useCommandPaletteStore((s) => s.setOpen);
  const [search, setSearch] = useState("");
  const tracks = useLibraryStore((s) => s.tracks);
  const playlists = usePlaylistStore((s) => s.playlists);

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.key.toLowerCase() === "k" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        useCommandPaletteStore.getState().toggle();
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  useEffect(() => {
    if (!open) setSearch("");
  }, [open]);

  const matchedTracks = useMemo(
    () => tracks.filter((t) => trackMatchesQuery(t, search)).slice(0, MAX_RESULTS_PER_GROUP),
    [tracks, search],
  );

  const albumGroups = useMemo(() => groupAlbums(tracks), [tracks]);
  const matchedAlbums = useMemo(() => {
    const q = search.trim().toLowerCase();
    const filtered = q
      ? albumGroups.filter(
          (g) => g.album.toLowerCase().includes(q) || g.artist.toLowerCase().includes(q),
        )
      : albumGroups;
    return filtered.slice(0, MAX_RESULTS_PER_GROUP);
  }, [albumGroups, search]);

  const artistGroups = useMemo(() => groupArtists(tracks), [tracks]);
  const matchedArtists = useMemo(() => {
    const q = search.trim().toLowerCase();
    const filtered = q ? artistGroups.filter((g) => g.artist.toLowerCase().includes(q)) : artistGroups;
    return filtered.slice(0, MAX_RESULTS_PER_GROUP);
  }, [artistGroups, search]);

  const matchedPlaylists = useMemo(() => {
    const q = search.trim().toLowerCase();
    const filtered = q ? playlists.filter((p) => p.name.toLowerCase().includes(q)) : playlists;
    return filtered.slice(0, MAX_RESULTS_PER_GROUP);
  }, [playlists, search]);

  function close() {
    setOpen(false);
  }

  function openTrack(track: (typeof tracks)[number]) {
    useQueueStore.getState().setQueue(tracks, track);
    close();
  }

  function openAlbum(album: string) {
    useSearchStore.getState().setQuery(album);
    useUiStore.getState().setView("albums");
    close();
  }

  function openArtist(artist: string) {
    useSearchStore.getState().setQuery(artist);
    useUiStore.getState().setView("artists");
    close();
  }

  function openPlaylist(id: number) {
    usePlaylistStore.getState().selectPlaylist(id);
    useUiStore.getState().setView("playlists");
    close();
  }

  const hasResults =
    matchedTracks.length > 0 ||
    matchedAlbums.length > 0 ||
    matchedArtists.length > 0 ||
    matchedPlaylists.length > 0;

  return (
    <Command.Dialog
      open={open}
      onOpenChange={setOpen}
      shouldFilter={false}
      label="Командная палитра"
      overlayClassName="fixed inset-0 z-50 bg-black/50"
      contentClassName="fixed left-1/2 top-24 z-50 w-full max-w-lg -translate-x-1/2 overflow-hidden rounded-xl bg-sidebar-background shadow-lg ring-1 ring-divider"
    >
      <div className="flex items-center gap-2 border-b border-divider px-3">
        <Search size={16} className="text-text-secondary" />
        <Command.Input
          value={search}
          onValueChange={setSearch}
          placeholder="Поиск треков, альбомов, исполнителей, плейлистов…"
          className="w-full bg-transparent py-3 text-sm text-text-primary outline-none placeholder:text-text-secondary"
        />
      </div>

      <Command.List className="max-h-[400px] overflow-y-auto p-2">
        {!hasResults && (
          <Command.Empty className="px-3 py-6 text-center text-sm text-text-secondary">
            Ничего не найдено
          </Command.Empty>
        )}

        {matchedTracks.length > 0 && (
          <Command.Group
            heading="Треки"
            className="px-2 py-1 text-xs font-medium text-text-secondary [&_[cmdk-group-heading]]:mb-1"
          >
            {matchedTracks.map((t) => (
              <Command.Item
                key={`track-${t.id}`}
                value={`track-${t.id}`}
                onSelect={() => openTrack(t)}
                className="flex cursor-default items-center gap-2 rounded-md px-2 py-2 text-sm text-text-primary data-[selected=true]:bg-card-hover"
              >
                <Music2 size={14} className="flex-shrink-0 text-text-secondary" />
                <span className="truncate">{t.title}</span>
                <span className="truncate text-xs text-text-secondary">
                  {t.artist ?? "Неизвестный исполнитель"}
                </span>
              </Command.Item>
            ))}
          </Command.Group>
        )}

        {matchedAlbums.length > 0 && (
          <Command.Group
            heading="Альбомы"
            className="px-2 py-1 text-xs font-medium text-text-secondary [&_[cmdk-group-heading]]:mb-1"
          >
            {matchedAlbums.map((g) => (
              <Command.Item
                key={`album-${g.key}`}
                value={`album-${g.key}`}
                onSelect={() => openAlbum(g.album)}
                className="flex cursor-default items-center gap-2 rounded-md px-2 py-2 text-sm text-text-primary data-[selected=true]:bg-card-hover"
              >
                <Disc3 size={14} className="flex-shrink-0 text-text-secondary" />
                <span className="truncate">{g.album}</span>
                <span className="truncate text-xs text-text-secondary">{g.artist}</span>
              </Command.Item>
            ))}
          </Command.Group>
        )}

        {matchedArtists.length > 0 && (
          <Command.Group
            heading="Исполнители"
            className="px-2 py-1 text-xs font-medium text-text-secondary [&_[cmdk-group-heading]]:mb-1"
          >
            {matchedArtists.map((g) => (
              <Command.Item
                key={`artist-${g.artist}`}
                value={`artist-${g.artist}`}
                onSelect={() => openArtist(g.artist)}
                className="flex cursor-default items-center gap-2 rounded-md px-2 py-2 text-sm text-text-primary data-[selected=true]:bg-card-hover"
              >
                <Mic2 size={14} className="flex-shrink-0 text-text-secondary" />
                <span className="truncate">{g.artist}</span>
              </Command.Item>
            ))}
          </Command.Group>
        )}

        {matchedPlaylists.length > 0 && (
          <Command.Group
            heading="Плейлисты"
            className="px-2 py-1 text-xs font-medium text-text-secondary [&_[cmdk-group-heading]]:mb-1"
          >
            {matchedPlaylists.map((p) => (
              <Command.Item
                key={`playlist-${p.id}`}
                value={`playlist-${p.id}`}
                onSelect={() => openPlaylist(p.id)}
                className="flex cursor-default items-center gap-2 rounded-md px-2 py-2 text-sm text-text-primary data-[selected=true]:bg-card-hover"
              >
                <ListMusic size={14} className="flex-shrink-0 text-text-secondary" />
                <span className="truncate">{p.name}</span>
              </Command.Item>
            ))}
          </Command.Group>
        )}
      </Command.List>
    </Command.Dialog>
  );
}
