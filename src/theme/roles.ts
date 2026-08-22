import type { Theme } from "../types";

export interface ThemeRole {
  key: keyof Theme;
  cssVar: string;
  label: string;
}

export const THEME_ROLES: ThemeRole[] = [
  { key: "background", cssVar: "--color-background", label: "Фон" },
  {
    key: "sidebarBackground",
    cssVar: "--color-sidebar-background",
    label: "Фон боковой панели",
  },
  {
    key: "playerBarBackground",
    cssVar: "--color-player-bar-background",
    label: "Фон панели плеера",
  },
  {
    key: "cardBackground",
    cssVar: "--color-card-background",
    label: "Фон карточек",
  },
  {
    key: "cardHover",
    cssVar: "--color-card-hover",
    label: "Карточка при наведении",
  },
  {
    key: "accentPrimary",
    cssVar: "--color-accent-primary",
    label: "Основной акцент",
  },
  {
    key: "accentSecondary",
    cssVar: "--color-accent-secondary",
    label: "Дополнительный акцент",
  },
  {
    key: "textPrimary",
    cssVar: "--color-text-primary",
    label: "Основной текст",
  },
  {
    key: "textSecondary",
    cssVar: "--color-text-secondary",
    label: "Второстепенный текст",
  },
  { key: "divider", cssVar: "--color-divider", label: "Разделители" },
  {
    key: "progressTrack",
    cssVar: "--color-progress-track",
    label: "Дорожка прогресса",
  },
  {
    key: "progressFill",
    cssVar: "--color-progress-fill",
    label: "Заполнение прогресса",
  },
  {
    key: "karaokeInactiveLine",
    cssVar: "--color-karaoke-inactive-line",
    label: "Караоке: неактивная строка",
  },
  {
    key: "karaokeActiveLine",
    cssVar: "--color-karaoke-active-line",
    label: "Караоке: активная строка",
  },
  {
    key: "karaokeActiveWordHighlight",
    cssVar: "--color-karaoke-active-word-highlight",
    label: "Караоке: подсветка слова",
  },
];

export function applyThemeToDom(theme: Theme) {
  const root = document.documentElement;
  for (const role of THEME_ROLES) {
    root.style.setProperty(role.cssVar, theme[role.key]);
  }
}
