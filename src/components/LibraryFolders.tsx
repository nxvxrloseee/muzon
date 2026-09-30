import { Folder, FolderPlus, RefreshCw, X } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import { useLibraryStore } from "../store/libraryStore";

function FolderRow({ path }: { path: string }) {
  const removeFolder = useLibraryStore((s) => s.removeFolder);
  // Two clicks rather than a dialog: removing drops favourites, play counts
  // and playlist places with the tracks, so it deserves a second look
  const [confirming, setConfirming] = useState(false);

  useEffect(() => {
    if (!confirming) return;
    const timer = setTimeout(() => setConfirming(false), 4000);
    return () => clearTimeout(timer);
  }, [confirming]);

  async function remove() {
    if (!confirming) {
      setConfirming(true);
      return;
    }
    try {
      const removed = await removeFolder(path);
      toast.success(`Папка убрана из библиотеки, треков: ${removed}`);
    } catch (e) {
      toast.error(`Не удалось убрать папку: ${String(e)}`);
    }
  }

  return (
    <div className="flex items-center gap-3 px-3 py-2">
      <Folder size={16} className="flex-shrink-0 text-text-secondary" />
      <span className="min-w-0 flex-1 truncate text-sm text-text-primary" title={path}>
        {path}
      </span>
      <button
        onClick={remove}
        className={`flex flex-shrink-0 items-center gap-1 rounded px-2 py-1 text-xs ${
          confirming
            ? "bg-accent-secondary/20 text-accent-secondary"
            : "text-text-secondary hover:bg-card-hover hover:text-text-primary"
        }`}
        title="Убрать из библиотеки (файлы останутся на диске)"
      >
        <X size={12} />
        {confirming ? "Точно убрать?" : "Убрать"}
      </button>
    </div>
  );
}

export function LibraryFolders() {
  const folders = useLibraryStore((s) => s.folders);
  const loadFolders = useLibraryStore((s) => s.loadFolders);
  const addFolder = useLibraryStore((s) => s.addFolder);
  const rescan = useLibraryStore((s) => s.rescan);
  const scanning = useLibraryStore((s) => s.scanning);

  useEffect(() => {
    void loadFolders();
  }, [loadFolders]);

  return (
    <div className="mb-8">
      <h2 className="mb-1 text-lg font-semibold text-text-primary">Папки фонотеки</h2>
      <p className="mb-4 text-sm text-text-secondary">
        Новые, изменённые и удалённые файлы подхватываются сами. Если папка недоступна
        (диск не подключён), её треки остаются в библиотеке.
      </p>

      {folders.length > 0 && (
        <div className="mb-3 flex flex-col divide-y divide-divider rounded-lg border border-divider">
          {folders.map((path) => (
            <FolderRow key={path} path={path} />
          ))}
        </div>
      )}

      <div className="flex flex-wrap gap-2">
        <button
          onClick={() => addFolder()}
          disabled={scanning}
          className="flex items-center gap-1.5 rounded-md bg-card-background px-3 py-1.5 text-xs text-text-primary hover:bg-card-hover disabled:opacity-50"
        >
          <FolderPlus size={14} />
          Добавить папку
        </button>
        <button
          onClick={() => rescan()}
          disabled={scanning || folders.length === 0}
          className="flex items-center gap-1.5 rounded-md bg-card-background px-3 py-1.5 text-xs text-text-primary hover:bg-card-hover disabled:opacity-50"
        >
          <RefreshCw size={14} className={scanning ? "animate-spin" : ""} />
          {scanning ? "Сканирование…" : "Пересканировать"}
        </button>
      </div>
    </div>
  );
}
