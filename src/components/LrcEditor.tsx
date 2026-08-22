import { Pause, Play } from "lucide-react";
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
import { lyricsApi } from "../api/lyrics";
import { usePlayerStore } from "../store/playerStore";
import type { Track } from "../types";

function formatLrcTime(secs: number): string {
  const m = Math.floor(secs / 60);
  const s = secs - m * 60;
  return `[${m.toString().padStart(2, "0")}:${s.toFixed(2).padStart(5, "0")}]`;
}

function stripTimestamps(line: string): string {
  return line.replace(/^(\[\d{1,2}:\d{2}(?:\.\d{1,3})?\])+/, "").trim();
}

interface TapLine {
  time: number;
  text: string;
}

export function LrcEditor({
  track,
  open,
  onOpenChange,
}: {
  track: Track | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { positionSecs, isPlaying, toggle, play } = usePlayerStore();
  const [mode, setMode] = useState<"text" | "tap">("text");
  const [text, setText] = useState("");
  const [tapLines, setTapLines] = useState<string[]>([]);
  const [tapIndex, setTapIndex] = useState(0);
  const [tapStamped, setTapStamped] = useState<TapLine[]>([]);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open || !track) return;
    lyricsApi.getLyricsSourceText(track.path).then(setText);
    setMode("text");
    setError(null);
  }, [open, track]);

  useEffect(() => {
    if (mode !== "tap") return;
    function onKeyDown(e: KeyboardEvent) {
      if (e.code === "Space") {
        e.preventDefault();
        tapCurrentLine();
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode, tapIndex, tapLines, positionSecs]);

  function startTapMode() {
    const lines = text
      .split("\n")
      .map(stripTimestamps)
      .filter((l) => l.length > 0);
    setTapLines(lines);
    setTapStamped([]);
    setTapIndex(0);
    setMode("tap");
  }

  function tapCurrentLine() {
    setTapIndex((i) => {
      if (i >= tapLines.length) return i;
      setTapStamped((prev) => [...prev, { time: positionSecs, text: tapLines[i] }]);
      return i + 1;
    });
  }

  function finishTapMode() {
    setText(tapStamped.map((l) => `${formatLrcTime(l.time)}${l.text}`).join("\n"));
    setMode("text");
  }

  function shiftAll(deltaSecs: number) {
    const shifted = text.split("\n").map((line) => {
      const match = line.match(/^\[(\d{1,2}):(\d{2}(?:\.\d{1,3})?)\](.*)$/);
      if (!match) return line;
      const totalSecs = Math.max(0, Number(match[1]) * 60 + Number(match[2]) + deltaSecs);
      return `${formatLrcTime(totalSecs)}${match[3]}`;
    });
    setText(shifted.join("\n"));
  }

  async function handleSave(storeInTag: boolean) {
    if (!track) return;
    setSaving(true);
    setError(null);
    try {
      await lyricsApi.saveLyrics(track.path, text, storeInTag);
      onOpenChange(false);
      toast.success(storeInTag ? "Текст сохранён в тег" : "Текст сохранён в .lrc файл");
    } catch (e) {
      setError(String(e));
      toast.error(`Не удалось сохранить текст: ${String(e)}`);
    } finally {
      setSaving(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>Текст песни{track ? `: ${track.title}` : ""}</DialogTitle>
        </DialogHeader>

        {mode === "text" ? (
          <div className="flex flex-col gap-3">
            <textarea
              value={text}
              onChange={(e) => setText(e.target.value)}
              placeholder={
                'Вставьте текст песни построчно, затем используйте режим "нажимай в такт", чтобы расставить тайминги.\n\nИли редактируйте LRC вручную:\n[00:12.50]Первая строка'
              }
              className="h-64 w-full resize-none rounded-md border border-divider bg-background p-3 font-mono text-xs text-text-primary outline-none"
              spellCheck={false}
            />

            <div className="flex flex-wrap items-center gap-2">
              <Button size="sm" variant="outline" onClick={startTapMode} disabled={!text.trim()}>
                Режим «нажимай в такт»
              </Button>
              <span className="text-xs text-text-secondary">Сдвиг таймингов:</span>
              <Button size="sm" variant="outline" onClick={() => shiftAll(-0.5)}>
                −0.5с
              </Button>
              <Button size="sm" variant="outline" onClick={() => shiftAll(0.5)}>
                +0.5с
              </Button>
            </div>

            {error && <p className="text-xs text-red-400">{error}</p>}
          </div>
        ) : (
          <div className="flex flex-col items-center gap-4 py-4">
            <p className="text-center text-xs text-text-secondary">
              Нажимайте кнопку (или пробел) точно в момент начала каждой строки
            </p>
            <p className="text-sm text-text-secondary">
              {tapIndex} / {tapLines.length}
            </p>

            <div className="flex min-h-16 flex-col items-center justify-center gap-1 text-center">
              <div className="text-lg font-semibold text-text-primary">
                {tapLines[tapIndex] ?? "Готово!"}
              </div>
              {tapLines[tapIndex + 1] && (
                <div className="text-sm text-text-secondary">{tapLines[tapIndex + 1]}</div>
              )}
            </div>

            <div className="flex items-center gap-3">
              <button
                onClick={() => (isPlaying ? toggle() : track && play(track))}
                className="flex h-10 w-10 items-center justify-center rounded-full bg-card-hover text-text-primary"
              >
                {isPlaying ? <Pause size={18} /> : <Play size={18} />}
              </button>
              <Button onClick={tapCurrentLine} disabled={tapIndex >= tapLines.length}>
                Метка ({positionSecs.toFixed(1)}с)
              </Button>
            </div>

            <div className="flex gap-2">
              <Button variant="outline" onClick={() => setMode("text")}>
                Отмена
              </Button>
              <Button onClick={finishTapMode} disabled={tapStamped.length === 0}>
                Готово, вернуться к редактированию
              </Button>
            </div>
          </div>
        )}

        {mode === "text" && (
          <DialogFooter>
            <Button variant="outline" onClick={() => onOpenChange(false)}>
              Отмена
            </Button>
            <Button variant="outline" disabled={saving} onClick={() => handleSave(false)}>
              Сохранить в .lrc файл
            </Button>
            <Button disabled={saving} onClick={() => handleSave(true)}>
              Сохранить в тег
            </Button>
          </DialogFooter>
        )}
      </DialogContent>
    </Dialog>
  );
}
