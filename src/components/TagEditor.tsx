import { open } from "@tauri-apps/plugin-dialog";
import { ImageUp, Music2 } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useTrackCover } from "../hooks/useTrackCover";
import { useLibraryStore } from "../store/libraryStore";
import type { Track } from "../types";

export function TagEditor({
  track,
  open: isOpen,
  onOpenChange,
}: {
  track: Track | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const updateTrackTags = useLibraryStore((s) => s.updateTrackTags);
  const existingCover = useTrackCover(track?.path);

  const [title, setTitle] = useState("");
  const [artist, setArtist] = useState("");
  const [album, setAlbum] = useState("");
  const [trackNo, setTrackNo] = useState("");
  const [newCoverPath, setNewCoverPath] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!track) return;
    setTitle(track.title);
    setArtist(track.artist ?? "");
    setAlbum(track.album ?? "");
    setTrackNo(track.track_no != null ? String(track.track_no) : "");
    setNewCoverPath(null);
    setError(null);
  }, [track]);

  async function pickCover() {
    const path = await open({
      multiple: false,
      filters: [{ name: "Изображение", extensions: ["jpg", "jpeg", "png"] }],
    });
    if (path && !Array.isArray(path)) setNewCoverPath(path);
  }

  async function handleSave() {
    if (!track) return;
    setSaving(true);
    setError(null);
    try {
      await updateTrackTags(track.path, {
        title: title.trim() || track.title,
        artist: artist.trim() || null,
        album: album.trim() || null,
        trackNo: trackNo.trim() ? Number(trackNo) : null,
        coverPath: newCoverPath,
      });
      onOpenChange(false);
      toast.success("Тег сохранён");
    } catch (e) {
      setError(String(e));
      toast.error(`Не удалось сохранить тег: ${String(e)}`);
    } finally {
      setSaving(false);
    }
  }

  return (
    <Dialog open={isOpen} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Редактировать тег</DialogTitle>
        </DialogHeader>

        <div className="flex gap-4">
          <button
            onClick={pickCover}
            className="group relative flex h-24 w-24 flex-shrink-0 items-center justify-center overflow-hidden rounded-md bg-card-background"
          >
            {existingCover ? (
              <img src={existingCover} alt="" className="h-full w-full object-cover" />
            ) : (
              <Music2 size={24} className="text-text-secondary/40" />
            )}
            <div className="absolute inset-0 flex items-center justify-center bg-black/50 opacity-0 transition-opacity group-hover:opacity-100">
              <ImageUp size={20} className="text-white" />
            </div>
          </button>

          <div className="flex flex-1 flex-col gap-3">
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="tag-title">Название</Label>
              <Input id="tag-title" value={title} onChange={(e) => setTitle(e.target.value)} />
            </div>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="tag-artist">Исполнитель</Label>
              <Input id="tag-artist" value={artist} onChange={(e) => setArtist(e.target.value)} />
            </div>
          </div>
        </div>

        <div className="grid grid-cols-[1fr_auto] gap-3">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="tag-album">Альбом</Label>
            <Input id="tag-album" value={album} onChange={(e) => setAlbum(e.target.value)} />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="tag-track-no">№ трека</Label>
            <Input
              id="tag-track-no"
              type="number"
              min={0}
              className="w-20"
              value={trackNo}
              onChange={(e) => setTrackNo(e.target.value)}
            />
          </div>
        </div>

        {newCoverPath && (
          <p className="text-xs text-text-secondary">
            Новая обложка: {newCoverPath.split("/").pop()}
          </p>
        )}
        {error && <p className="text-xs text-red-400">{error}</p>}

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Отмена
          </Button>
          <Button onClick={handleSave} disabled={saving}>
            {saving ? "Сохранение…" : "Сохранить"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
