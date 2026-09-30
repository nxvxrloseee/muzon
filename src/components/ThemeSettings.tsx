import { HexColorPicker } from "react-colorful";
import { useState } from "react";
import { CloudSettings } from "./CloudSettings";
import { LibraryFolders } from "./LibraryFolders";
import { ScrobbleSettings } from "./ScrobbleSettings";
import { useAppearanceStore } from "../store/appearanceStore";
import { useHotkeysStore } from "../store/hotkeysStore";
import { useThemeStore } from "../store/themeStore";
import { THEME_ROLES } from "../theme/roles";
import type { Hotkeys, Theme } from "../types";

function RoleSwatch({ roleKey, label }: { roleKey: keyof Theme; label: string }) {
  const [open, setOpen] = useState(false);
  const value = useThemeStore((s) => s.theme?.[roleKey] ?? "#000000");
  const setColor = useThemeStore((s) => s.setColor);
  const fromSystem = useThemeStore((s) => s.source === "system");

  return (
    <div
      className={`relative flex items-center justify-between gap-3 rounded-md px-3 py-2 hover:bg-card-hover ${
        fromSystem ? "opacity-60" : ""
      }`}
    >
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

function SystemThemeToggle() {
  const source = useThemeStore((s) => s.source);
  const available = useThemeStore((s) => s.systemAvailable);
  const setSource = useThemeStore((s) => s.setSource);
  const [error, setError] = useState<string | null>(null);
  const on = source === "system";

  async function toggle() {
    setError(null);
    try {
      await setSource(on ? "manual" : "system");
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="mb-4 rounded-md bg-card-background p-3">
      <div className="flex items-center justify-between gap-3">
        <div>
          <div className="text-sm text-text-primary">Цвета системы</div>
          <div className="text-xs text-text-secondary">
            {available
              ? "Палитра берётся из темы рабочего стола и меняется вместе с обоями"
              : "Палитра рабочего стола не найдена (caelestia или rice)"}
          </div>
        </div>
        <button
          onClick={toggle}
          disabled={!available && !on}
          aria-pressed={on}
          className={`h-6 w-11 shrink-0 rounded-full transition-colors disabled:opacity-40 ${
            on ? "bg-accent-primary" : "bg-divider"
          }`}
        >
          <span
            className={`block h-5 w-5 rounded-full bg-background transition-transform ${
              on ? "translate-x-[22px]" : "translate-x-[2px]"
            }`}
          />
        </button>
      </div>
      {on && (
        <p className="mt-2 text-xs text-text-secondary">
          Правка любого цвета ниже вернёт ручную палитру.
        </p>
      )}
      {error && <p className="mt-2 text-xs text-accent-secondary">{error}</p>}
    </div>
  );
}

/** Mirrors `MIN_BACKGROUND_OPACITY` in `domain/appearance.rs`. */
const MIN_BACKGROUND_OPACITY = 0.3;

function WindowTransparency() {
  const appearance = useAppearanceStore((s) => s.appearance);
  const windowTransparent = useAppearanceStore((s) => s.windowTransparent);
  const setTransparentWindow = useAppearanceStore((s) => s.setTransparentWindow);
  const setBackgroundOpacity = useAppearanceStore((s) => s.setBackgroundOpacity);
  if (!appearance) return null;

  const on = appearance.transparentWindow;
  const needsRestart = on !== windowTransparent;

  return (
    <div className="mb-4 rounded-md bg-card-background p-3">
      <div className="flex items-center justify-between gap-3">
        <div>
          <div className="text-sm text-text-primary">Прозрачное окно</div>
          <div className="text-xs text-text-secondary">
            Сквозь фон видно рабочий стол. Выключенная прозрачность экономит работу композитора.
          </div>
        </div>
        <button
          onClick={() => setTransparentWindow(!on)}
          aria-pressed={on}
          className={`h-6 w-11 shrink-0 rounded-full transition-colors ${
            on ? "bg-accent-primary" : "bg-divider"
          }`}
        >
          <span
            className={`block h-5 w-5 rounded-full bg-background transition-transform ${
              on ? "translate-x-[22px]" : "translate-x-[2px]"
            }`}
          />
        </button>
      </div>
      {needsRestart && (
        <p className="mt-2 text-xs text-accent-secondary">
          Применится после перезапуска Muzon.
        </p>
      )}
      {windowTransparent && (
        <label className="mt-3 flex items-center gap-3">
          <span className="shrink-0 text-xs text-text-secondary">Непрозрачность фона</span>
          <input
            type="range"
            min={MIN_BACKGROUND_OPACITY}
            max={1}
            step={0.01}
            value={appearance.backgroundOpacity}
            onChange={(e) => setBackgroundOpacity(Number(e.target.value))}
            className="flex-1"
          />
          <span className="w-10 shrink-0 text-right text-xs tabular-nums text-text-primary">
            {Math.round(appearance.backgroundOpacity * 100)}%
          </span>
        </label>
      )}
    </div>
  );
}

function CloseToTray() {
  const appearance = useAppearanceStore((s) => s.appearance);
  const setCloseToTray = useAppearanceStore((s) => s.setCloseToTray);
  if (!appearance) return null;
  const on = appearance.closeToTray;

  return (
    <div className="mb-4 rounded-md bg-card-background p-3">
      <div className="flex items-center justify-between gap-3">
        <div>
          <div className="text-sm text-text-primary">Сворачивать в трей при закрытии</div>
          <div className="text-xs text-text-secondary">
            Музыка продолжит играть; вернуть окно — из меню иконки в трее. Нужна панель,
            которая показывает трей (например, waybar или caelestia).
          </div>
        </div>
        <button
          onClick={() => setCloseToTray(!on)}
          aria-pressed={on}
          className={`h-6 w-11 shrink-0 rounded-full transition-colors ${
            on ? "bg-accent-primary" : "bg-divider"
          }`}
        >
          <span
            className={`block h-5 w-5 rounded-full bg-background transition-transform ${
              on ? "translate-x-[22px]" : "translate-x-[2px]"
            }`}
          />
        </button>
      </div>
    </div>
  );
}

export function ThemeSettings() {
  const resetToDefault = useThemeStore((s) => s.resetToDefault);
  const exportToFile = useThemeStore((s) => s.exportToFile);
  const importFromFile = useThemeStore((s) => s.importFromFile);

  return (
    <div className="mx-auto max-w-xl p-6">
      <LibraryFolders />

      <h1 className="mb-1 text-lg font-semibold text-text-primary">Тема оформления</h1>
      <p className="mb-4 text-sm text-text-secondary">
        Изменения применяются мгновенно и сохраняются автоматически.
      </p>

      <SystemThemeToggle />
      <WindowTransparency />
      <CloseToTray />

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

      <CloudSettings />

      <ScrobbleSettings />

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
