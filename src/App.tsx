import { useEffect, useRef } from "react";
import { Toaster } from "sonner";
import { AlbumsView } from "./components/AlbumsView";
import { ArtistsView } from "./components/ArtistsView";
import { CommandPalette } from "./components/CommandPalette";
import { NowPlaying } from "./components/NowPlaying";
import { PlayerBar } from "./components/PlayerBar";
import { PlaylistsView } from "./components/PlaylistsView";
import { Sidebar } from "./components/Sidebar";
import { ThemeSettings } from "./components/ThemeSettings";
import { TrackList } from "./components/TrackList";
import { WindowControls } from "./components/WindowControls";
import { useFlushSettingsOnClose } from "./hooks/useFlushSettingsOnClose";
import { useHotkeys } from "./hooks/useHotkeys";
import { useLenis } from "./hooks/useLenis";
import { useMprisEvents } from "./hooks/useMprisEvents";
import { useWindowControls } from "./hooks/useWindowControls";
import { useHotkeysStore } from "./store/hotkeysStore";
import { useLibraryStore } from "./store/libraryStore";
import { usePlayerStore } from "./store/playerStore";
import { restoreSession, startSessionSync } from "./store/sessionStore";
import { useThemeStore } from "./store/themeStore";
import { useUiStore } from "./store/uiStore";

function App() {
  const refreshTracks = useLibraryStore((s) => s.refreshTracks);
  const ensureSubscribed = usePlayerStore((s) => s.ensureSubscribed);
  const initTheme = useThemeStore((s) => s.init);
  const initHotkeys = useHotkeysStore((s) => s.init);
  const view = useUiStore((s) => s.view);
  const showWindowControls = useWindowControls();
  const mainRef = useRef<HTMLElement>(null);
  const mainContentRef = useRef<HTMLDivElement>(null);

  useHotkeys();
  useMprisEvents();
  useLenis(mainRef, mainContentRef);
  useFlushSettingsOnClose();

  useEffect(() => {
    ensureSubscribed();
    initTheme();
    initHotkeys();

    // The saved session holds track *paths*, so it can only be turned back into
    // a queue once the library those paths refer to is in memory.
    let cancelled = false;
    let stopSessionSync: (() => void) | undefined;
    void (async () => {
      await refreshTracks();
      await restoreSession();
      if (cancelled) return;
      stopSessionSync = startSessionSync();
    })();

    return () => {
      cancelled = true;
      stopSessionSync?.();
    };
  }, [refreshTracks, ensureSubscribed, initTheme, initHotkeys]);

  if (view === "now-playing") {
    return (
      <div className="relative h-screen w-screen overflow-hidden bg-background text-text-primary">
        <NowPlaying />
        {/* Overlaid instead of given a bar of its own: the full-bleed gradient
            is the whole point of this view, and it sits on the right so it
            can't swallow the "Свернуть" button in the opposite corner. */}
        {showWindowControls && <WindowControls className="absolute right-0 top-0 z-20 h-11 pl-8 pr-2" />}
      </div>
    );
  }

  return (
    <div className="flex h-screen w-screen flex-col overflow-hidden bg-background text-text-primary">
      {showWindowControls && (
        <WindowControls className="h-9 flex-shrink-0 justify-end border-b border-divider px-2" />
      )}
      <div className="flex flex-1 overflow-hidden">
        <Sidebar />
        <main ref={mainRef} className="flex-1 overflow-y-auto">
          <div ref={mainContentRef} className="h-full">
            {view === "settings" && <ThemeSettings />}
            {view === "playlists" && <PlaylistsView />}
            {view === "albums" && <AlbumsView />}
            {view === "artists" && <ArtistsView />}
            {view === "library" && <TrackList />}
          </div>
        </main>
      </div>
      <PlayerBar />
      <CommandPalette />
      <Toaster
        position="top-right"
        theme="dark"
        toastOptions={{
          style: {
            background: "var(--color-card-background)",
            color: "var(--color-text-primary)",
            border: "1px solid var(--color-divider)",
          },
        }}
      />
    </div>
  );
}

export default App;
