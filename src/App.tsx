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
import { useFlushSettingsOnClose } from "./hooks/useFlushSettingsOnClose";
import { useHotkeys } from "./hooks/useHotkeys";
import { useLenis } from "./hooks/useLenis";
import { useMprisEvents } from "./hooks/useMprisEvents";
import { useHotkeysStore } from "./store/hotkeysStore";
import { useLibraryStore } from "./store/libraryStore";
import { usePlayerStore } from "./store/playerStore";
import { useThemeStore } from "./store/themeStore";
import { useUiStore } from "./store/uiStore";

function App() {
  const refreshTracks = useLibraryStore((s) => s.refreshTracks);
  const ensureSubscribed = usePlayerStore((s) => s.ensureSubscribed);
  const initTheme = useThemeStore((s) => s.init);
  const initHotkeys = useHotkeysStore((s) => s.init);
  const view = useUiStore((s) => s.view);
  const mainRef = useRef<HTMLElement>(null);
  const mainContentRef = useRef<HTMLDivElement>(null);

  useHotkeys();
  useMprisEvents();
  useLenis(mainRef, mainContentRef);
  useFlushSettingsOnClose();

  useEffect(() => {
    refreshTracks();
    ensureSubscribed();
    initTheme();
    initHotkeys();
  }, [refreshTracks, ensureSubscribed, initTheme, initHotkeys]);

  if (view === "now-playing") {
    return (
      <div className="h-screen w-screen overflow-hidden bg-background text-text-primary">
        <NowPlaying />
      </div>
    );
  }

  return (
    <div className="flex h-screen w-screen flex-col overflow-hidden bg-background text-text-primary">
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
