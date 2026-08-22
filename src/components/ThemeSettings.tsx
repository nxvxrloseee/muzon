import { HexColorPicker } from "react-colorful";
import { useState } from "react";
import { useHotkeysStore } from "../store/hotkeysStore";
import { useThemeStore } from "../store/themeStore";
import { THEME_ROLES } from "../theme/roles";
import type { Hotkeys, Theme } from "../types";

function RoleSwatch({ roleKey, label }: { roleKey: keyof Theme; label: string }) {
  const [open, setOpen] = useState(false);
  const value = useThemeStore((s) => s.theme?.[roleKey] ?? "#000000");
  const setColor = useThemeStore((s) => s.setColor);

  return (
    <div className="relative flex items-center justify-between gap-3 rounded-md px-3 py-2 hover:bg-card-hover">
      <span className="text-sm text-text-secondary">{label}</span>
      <div className="flex items-center gap-2">
        <input
          value={value}
          onChange={(e) => setColor(roleKey, e.target.value)}
          className="w-24 rounded border border-divider bg-transparent px-2 py-1 text-xs text-text-primary"
        />
        <button
          onClick={() => setOpen((v) => !v)}
          className="h-7 w-7 rounded-full border border-divider"
          style={{ backgroundColor: value }}
          aria-label={`Выбрать цвет: ${label}`}
        />
      </div>

      {open && (
        <div className="absolute right-0 top-full z-10 mt-2 rounded-md border border-divider bg-card-background p-3 shadow-lg">
          <HexColorPicker color={value} onChange={(c) => setColor(roleKey, c)} />
          <button
            onClick={() => setOpen(false)}
            className="mt-2 w-full rounded bg-accent-primary/20 py-1 text-xs text-text-primary"
          >
            Готово
          </button>
        </div>
      )}
    </div>
  );
}

function formatKeyLabel(key: string): string {
  if (key === " ") return "Пробел";
  return key;
}

const HOTKEY_LABELS: Record<keyof Hotkeys, string> = {
  playPause: "Пауза / воспроизведение",
  seekForward: "Перемотка вперёд",
  seekBackward: "Перемотка назад",
  volumeUp: "Громкость выше",
  volumeDown: "Громкость ниже",
  nextTrack: "Следующий трек",
  previousTrack: "Предыдущий трек",
  closeNowPlaying: "Свернуть Now Playing",
};

function HotkeyRow({ action }: { action: keyof Hotkeys }) {
  const [listening, setListening] = useState(false);
  const value = useHotkeysStore((s) => s.bindings[action]);
  const setBinding = useHotkeysStore((s) => s.setBinding);

  function startListening() {
    setListening(true);
    function onKey(e: KeyboardEvent) {
      e.preventDefault();
      window.removeEventListener("keydown", onKey, true);
      setListening(false);
      if (e.key !== "Escape") setBinding(action, e.key);
    }
    window.addEventListener("keydown", onKey, true);
  }

  return (
    <div className="flex items-center justify-between gap-3 rounded-md px-3 py-2 hover:bg-card-hover">
      <span className="text-sm text-text-secondary">{HOTKEY_LABELS[action]}</span>
      <button
        onClick={startListening}
        className={`min-w-24 rounded border px-2 py-1 text-xs ${
          listening
            ? "border-accent-primary text-accent-primary"
            : "border-divider text-text-primary"
        }`}
      >
        {listening ? "Нажмите клавишу…" : formatKeyLabel(value)}
      </button>
    </div>
  );
}

export function ThemeSettings() {
  const resetToDefault = useThemeStore((s) => s.resetToDefault);
  const exportToFile = useThemeStore((s) => s.exportToFile);
  const importFromFile = useThemeStore((s) => s.importFromFile);

  return (
    <div className="mx-auto max-w-xl p-6">
      <h1 className="mb-1 text-lg font-semibold text-text-primary">Тема оформления</h1>
      <p className="mb-4 text-sm text-text-secondary">
        Изменения применяются мгновенно и сохраняются автоматически.
      </p>

      <div className="mb-4 flex flex-wrap gap-2">
        <button
          onClick={() => resetToDefault("dark")}
          className="rounded-md bg-card-background px-3 py-1.5 text-xs text-text-primary hover:bg-card-hover"
        >
          Сбросить на тёмную
        </button>
        <button
          onClick={() => resetToDefault("light")}
          className="rounded-md bg-card-background px-3 py-1.5 text-xs text-text-primary hover:bg-card-hover"
        >
          Сбросить на светлую
        </button>
        <button
          onClick={() => exportToFile()}
          className="rounded-md bg-card-background px-3 py-1.5 text-xs text-text-primary hover:bg-card-hover"
        >
          Экспорт в JSON
        </button>
        <button
          onClick={() => importFromFile()}
          className="rounded-md bg-card-background px-3 py-1.5 text-xs text-text-primary hover:bg-card-hover"
        >
          Импорт из JSON
        </button>
      </div>

      <div className="mb-8 flex flex-col divide-y divide-divider rounded-lg border border-divider">
        {THEME_ROLES.map((role) => (
          <RoleSwatch key={role.key} roleKey={role.key} label={role.label} />
        ))}
      </div>

      <h2 className="mb-1 text-lg font-semibold text-text-primary">Горячие клавиши</h2>
      <p className="mb-4 text-sm text-text-secondary">
        Нажмите на клавишу и введите новое сочетание.
      </p>
      <div className="flex flex-col divide-y divide-divider rounded-lg border border-divider">
        {(Object.keys(HOTKEY_LABELS) as (keyof Hotkeys)[]).map((action) => (
          <HotkeyRow key={action} action={action} />
        ))}
      </div>
    </div>
  );
}
