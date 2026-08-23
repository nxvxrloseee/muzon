import {
  Disc3,
  FolderPlus,
  Library,
  ListMusic,
  Mic2,
  Search,
  Settings,
} from "lucide-react";
import { useCommandPaletteStore } from "../store/commandPaletteStore";
import { useLibraryStore } from "../store/libraryStore";
import { useSearchStore } from "../store/searchStore";
import { useUiStore, type ViewName } from "../store/uiStore";

const NAV_ITEMS: { view: ViewName; label: string; icon: typeof Library }[] = [
  { view: "library", label: "Библиотека", icon: Library },
  { view: "albums", label: "Альбомы", icon: Disc3 },
  { view: "artists", label: "Исполнители", icon: Mic2 },
  { view: "playlists", label: "Плейлисты", icon: ListMusic },
  { view: "settings", label: "Настройки", icon: Settings },
];

export function Sidebar() {
  // Individual selectors, not whole-store subscriptions: the sidebar used to
  // re-render on every change to the tracks array (a favourite toggle, a tempo
  // drag) despite rendering none of it.
  const addFolder = useLibraryStore((s) => s.addFolder);
  const scanning = useLibraryStore((s) => s.scanning);
  const error = useLibraryStore((s) => s.error);
  const view = useUiStore((s) => s.view);
  const setView = useUiStore((s) => s.setView);
  const query = useSearchStore((s) => s.query);
  const setQuery = useSearchStore((s) => s.setQuery);
  const openPalette = useCommandPaletteStore((s) => s.setOpen);

  return (
    <aside className="flex h-full w-60 flex-col gap-4 border-r border-divider bg-sidebar-background p-4">
      <div className="flex items-center gap-2 px-2 text-lg font-semibold text-text-primary">
        <Library size={20} />
        Muzon
      </div>

      <div className="relative">
        <Search
          size={14}
          className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-text-secondary"
        />
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Поиск…"
          className="w-full rounded-md border border-divider bg-card-background py-1.5 pl-8 pr-14 text-sm text-text-primary outline-none placeholder:text-text-secondary focus:border-accent-primary"
        />
        {!query && (
          <button
            onClick={() => openPalette(true)}
            title="Глобальный поиск по всей библиотеке"
            className="absolute right-2 top-1/2 flex -translate-y-1/2 items-center gap-0.5 rounded border border-divider bg-sidebar-background px-1.5 py-0.5 text-[10px] font-medium text-text-secondary hover:bg-card-hover hover:text-text-primary"
          >
            Ctrl+K
          </button>
        )}
      </div>

      <nav className="flex flex-col gap-1 text-sm">
        {NAV_ITEMS.map(({ view: itemView, label, icon: Icon }) => (
          <button
            key={itemView}
            onClick={() => setView(itemView)}
            className={`flex items-center gap-2 rounded-md px-3 py-2 text-left ${
              view === itemView
                ? "bg-card-background font-medium text-text-primary"
                : "text-text-secondary hover:bg-card-hover"
            }`}
          >
            <Icon size={16} />
            {label}
          </button>
        ))}
      </nav>

      <button
        onClick={() => addFolder()}
        disabled={scanning}
        className="mt-auto flex items-center justify-center gap-2 rounded-md bg-card-background px-3 py-2 text-sm text-text-primary hover:bg-card-hover disabled:opacity-50"
      >
        <FolderPlus size={16} />
        {scanning ? "Сканирование…" : "Добавить папку"}
      </button>

      {error && <p className="text-xs text-red-400">{error}</p>}
    </aside>
  );
}
