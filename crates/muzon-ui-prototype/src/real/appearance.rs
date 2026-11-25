//! Native-only preferences. No desktop configuration is changed.
use super::{message::Message, preferences::Store, state::RealApp, style as st};
use iced::{
    widget::{button, checkbox, column, row, slider, text, text_input},
    Element, Length,
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    #[default]
    Dark,
    Light,
    System,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Preferences {
    pub mode: Mode,
    pub accent: String,
    pub transparent_window: bool,
    pub background_opacity: f32,
    pub close_to_tray: bool,
    pub spectrum: bool,
    pub theme_file: String,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            mode: Mode::Dark,
            accent: String::new(),
            transparent_window: false,
            background_opacity: 0.85,
            close_to_tray: true,
            spectrum: false,
            theme_file: String::new(),
        }
    }
}
#[derive(Debug, Clone)]
pub enum Change {
    Mode(Mode),
    Accent(String),
    Transparent(bool),
    Opacity(f32),
    CloseToTray(bool),
    Spectrum(bool),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stamp {
    modified: Option<SystemTime>,
    len: u64,
}
#[derive(Debug, Clone)]
pub struct ThemeSource {
    pub stamp: Stamp,
    pub colors: Option<[u32; 12]>,
}
pub struct Appearance {
    pub prefs: Preferences,
    pub running_transparent: bool,
    pub accent_input: String,
    pub file_input: String,
    pub error: Option<String>,
    pub dirty: bool,
    pub system_light: bool,
    pub generation: u64,
    pub stamp: Option<Stamp>,
    custom: Option<[u32; 12]>,
    revision: u64,
    store: Store,
}
impl Appearance {
    pub fn new(profile: &Path) -> Self {
        let path = profile.join("config/appearance.json");
        let mut prefs: Preferences = std::fs::read(&path)
            .ok()
            .and_then(|v| serde_json::from_slice(&v).ok())
            .unwrap_or_default();
        if !prefs.background_opacity.is_finite() {
            prefs.background_opacity = 0.85;
        }
        prefs.background_opacity = prefs.background_opacity.clamp(0.3, 1.0);
        if !prefs.accent.is_empty() && parse_hex(&prefs.accent).is_none() {
            prefs.accent.clear();
        }
        let accent_input = if prefs.accent.is_empty() {
            "#7c5cff".into()
        } else {
            prefs.accent.clone()
        };
        let file_input = prefs.theme_file.clone();
        Self {
            running_transparent: prefs.transparent_window,
            prefs,
            accent_input,
            file_input,
            error: None,
            dirty: false,
            system_light: false,
            generation: 0,
            stamp: None,
            custom: None,
            revision: 0,
            store: Store::new(path),
        }
    }
    pub fn apply(&self) {
        let light = self.prefs.mode == Mode::Light
            || (self.prefs.mode == Mode::System && self.system_light);
        let mut colors = self
            .custom
            .unwrap_or(if light { st::LIGHT } else { st::DARK });
        if let Some(accent) = parse_hex(&self.prefs.accent) {
            colors[6] = accent;
            colors[11] = accent;
        }
        st::configure(
            colors,
            if self.running_transparent && self.prefs.transparent_window {
                self.prefs.background_opacity
            } else {
                1.0
            },
        );
    }
    pub fn change(&mut self, change: Change) -> Result<(), String> {
        match change {
            Change::Mode(mode) => {
                self.prefs.mode = mode;
                self.generation = self.generation.wrapping_add(1);
                self.stamp = None;
                if self.prefs.theme_file.is_empty() {
                    self.custom = None;
                }
            }
            Change::Accent(value) => {
                let color = parse_hex(&value).ok_or("Введите цвет #RRGGBB")?;
                self.prefs.accent = format!("#{color:06x}");
                self.accent_input = self.prefs.accent.clone();
            }
            Change::Transparent(value) => self.prefs.transparent_window = value,
            Change::Opacity(value) => {
                if !value.is_finite() {
                    return Err("Некорректная непрозрачность".into());
                }
                self.prefs.background_opacity = value.clamp(0.3, 1.0);
            }
            Change::CloseToTray(value) => self.prefs.close_to_tray = value,
            Change::Spectrum(value) => self.prefs.spectrum = value,
        }
        self.mark_dirty();
        self.apply();
        Ok(())
    }
    pub fn mark_dirty(&mut self) {
        self.revision += 1;
        self.dirty = true;
        self.error = None;
    }
    pub fn set_source(&mut self, result: Result<ThemeSource, String>) {
        match result {
            Ok(source) => {
                self.stamp = Some(source.stamp);
                if let Some(colors) = source.colors {
                    self.custom = Some(colors);
                }
                self.error = None;
                self.apply();
            }
            Err(error) => self.error = Some(error),
        }
    }
    pub fn import(&mut self) {
        self.prefs.theme_file = self.file_input.trim().to_owned();
        self.generation = self.generation.wrapping_add(1);
        self.stamp = None;
        if self.prefs.theme_file.is_empty() {
            self.custom = None;
        }
        self.mark_dirty();
        self.apply();
    }
    pub fn flush(&self) -> (Store, u64, serde_json::Value) {
        (
            self.store.clone(),
            self.revision,
            serde_json::to_value(&self.prefs).unwrap(),
        )
    }
    pub fn saved(&mut self, revision: u64, result: Result<(), String>) {
        if revision == self.revision {
            match result {
                Ok(()) => self.dirty = false,
                Err(error) => self.error = Some(error),
            }
        }
    }
}
pub fn parse_hex(input: &str) -> Option<u32> {
    let value = input.trim().trim_start_matches('#');
    if !matches!(value.len(), 6 | 8) {
        return None;
    }
    if !value.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(value.get(..6)?, 16).ok()
}
fn color(value: &serde_json::Value) -> Option<u32> {
    value
        .as_str()
        .and_then(parse_hex)
        .or_else(|| {
            value
                .get("color")
                .and_then(|v| v.as_str())
                .and_then(parse_hex)
        })
        .or_else(|| {
            value
                .get("hex")
                .and_then(|v| v.as_str())
                .and_then(parse_hex)
        })
}
/// The desktop palette is an explicit, read-only source, never a preference override.
pub fn ryoku_path() -> Option<PathBuf> {
    palette_path(
        std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from),
        std::env::var_os("HOME").map(PathBuf::from),
    )
}
fn palette_path(cache: Option<PathBuf>, home: Option<PathBuf>) -> Option<PathBuf> {
    cache
        .filter(|p| p.is_absolute())
        .or_else(|| home.filter(|p| p.is_absolute()).map(|p| p.join(".cache")))
        .map(|p| p.join("ryoku/colors.json"))
}
pub fn read_source(
    path: PathBuf,
    previous: Option<Stamp>,
    light: bool,
) -> Result<ThemeSource, String> {
    (|| -> anyhow::Result<ThemeSource> {
        let metadata = std::fs::metadata(&path)?;
        anyhow::ensure!(
            metadata.len() <= 2 * 1024 * 1024,
            "Файл темы превышает 2 МиБ"
        );
        let stamp = Stamp {
            modified: metadata.modified().ok(),
            len: metadata.len(),
        };
        if previous == Some(stamp) {
            return Ok(ThemeSource {
                stamp,
                colors: None,
            });
        }
        let value: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
        let value = value.get("colors").unwrap_or(&value);
        let roles: [&[&str]; 12] = [
            &["background", "surface"],
            &["sidebarBackground", "surface_container_low"],
            &["playerBarBackground", "surface_container"],
            &["cardBackground", "surface_container_high"],
            &["cardHover", "surface_container_highest"],
            &["accentSecondary", "secondary"],
            &["accentPrimary", "primary"],
            &["textPrimary", "on_surface"],
            &["textSecondary", "on_surface_variant"],
            &["divider", "outline_variant"],
            &["progressTrack", "surface_container_highest"],
            &["progressFill", "primary"],
        ];
        let picks: Vec<Option<u32>> = roles
            .iter()
            .map(|keys| keys.iter().find_map(|key| value.get(key).and_then(color)))
            .collect();
        anyhow::ensure!(
            picks.iter().any(Option::is_some),
            "В JSON нет распознанных цветов темы"
        );
        let light = picks[0]
            .map(|rgb| {
                (((rgb >> 16) & 255) * 2126 + ((rgb >> 8) & 255) * 7152 + (rgb & 255) * 722)
                    > 128 * 10000
            })
            .unwrap_or(light);
        let mut colors = if light { st::LIGHT } else { st::DARK };
        for (i, pick) in picks.into_iter().enumerate() {
            if let Some(rgb) = pick {
                colors[i] = rgb;
            }
        }
        Ok(ThemeSource {
            stamp,
            colors: Some(colors),
        })
    })()
    .map_err(|e| e.to_string())
}
pub async fn system_light() -> Option<bool> {
    use mpris_server::zbus;
    tokio::time::timeout(std::time::Duration::from_millis(700), async {
        let connection = zbus::Connection::session().await.ok()?;
        let proxy = zbus::Proxy::new(
            &connection,
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Settings",
        )
        .await
        .ok()?;
        let value: zbus::zvariant::OwnedValue = proxy
            .call("Read", &("org.freedesktop.appearance", "color-scheme"))
            .await
            .ok()?;
        Some(u32::try_from(value).ok()? == 2)
    })
    .await
    .ok()
    .flatten()
}
pub fn view(app: &RealApp) -> Element<'_, Message> {
    let appearance = &app.appearance;
    let prefs = &appearance.prefs;
    let mut modes = row![].spacing(8);
    for (label, mode) in [
        ("Тёмная", Mode::Dark),
        ("Светлая", Mode::Light),
        ("Системная", Mode::System),
    ] {
        modes = modes.push(
            button(label)
                .padding([9, 14])
                .style(if prefs.mode == mode {
                    st::primary
                } else {
                    st::secondary
                })
                .on_press(Message::AppearanceChanged(Change::Mode(mode))),
        );
    }
    let mut accents = row![].spacing(8);
    for hex in ["#7c5cff", "#c43e91", "#168a81", "#276fd4", "#bd7018"] {
        let value = hex.to_owned();
        accents = accents.push(
            button(text(hex).size(12))
                .padding([7, 10])
                .style(st::secondary)
                .on_press(Message::AppearanceChanged(Change::Accent(value))),
        );
    }
    let mut content=column![text("Внешний вид").size(28),modes,text("Акцент").size(16),accents,
        row![text_input("#RRGGBB",&appearance.accent_input).padding(9).style(st::input).on_input(Message::AppearanceAccentInput),button("Применить цвет").padding(9).style(st::secondary).on_press(Message::AppearanceAccentApply)].spacing(10),
        checkbox("Прозрачное окно",prefs.transparent_window).on_toggle(|v|Message::AppearanceChanged(Change::Transparent(v))),
        text(format!("Непрозрачность: {:.0}%",prefs.background_opacity*100.)).size(13),slider(0.3..=1.,prefs.background_opacity,|v|Message::AppearanceChanged(Change::Opacity(v))).step(0.01_f32).style(st::progress),
        checkbox("Закрывать окно в трей",prefs.close_to_tray).on_toggle(|v|Message::AppearanceChanged(Change::CloseToTray(v))),
        checkbox("Спектр на экране «Сейчас играет»",prefs.spectrum).on_toggle(|v|Message::AppearanceChanged(Change::Spectrum(v))),
        text("JSON-тема / Material You").size(16),row![text_input("Путь к JSON с цветами",&appearance.file_input).padding(9).style(st::input).on_input(Message::AppearanceFileInput),button("Подключить").padding(9).style(st::secondary).on_press(Message::AppearanceImport)].spacing(10),
        button("Использовать тему Ryoku").padding(9).style(st::secondary).on_press_maybe(ryoku_path().map(|_|Message::AppearanceRyoku)),
        text("Подключённый файл только читается; его изменения подхватываются автоматически. Очистите путь и нажмите «Подключить», чтобы вернуть встроенную тему.").size(12).color(st::muted()),
        button("Повторить сохранение").padding(9).style(st::quiet).on_press_maybe(appearance.dirty.then_some(Message::AppearanceSave))].spacing(16);
    if prefs.transparent_window != appearance.running_transparent {
        content = content.push(
            text("Изменение прозрачности окна завершится после перезапуска.")
                .size(13)
                .color(st::accent()),
        );
    }
    if let Some(error) = &appearance.error {
        content = content.push(text(error).size(12).color(iced::color!(0xff9696)));
    }
    st::scrollable(content)
        .height(Length::Fill)
        .style(st::scroll)
        .into()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_preferences_and_json_colors_are_bounded() {
        let dir = muzon_core::testing::scratch_dir("native-appearance");
        let mut app = Appearance::new(&dir);
        assert!(!app.running_transparent);
        app.change(Change::Opacity(0.)).unwrap();
        assert_eq!(app.prefs.background_opacity, 0.3);
        assert!(app.change(Change::Accent("no".into())).is_err());
        let path = dir.join("theme.json");
        std::fs::write(&path, r##"{"surface":"#ffffff","primary":"#123456"}"##).unwrap();
        let source = read_source(path.clone(), None, false).unwrap();
        assert_eq!(source.colors.unwrap()[6], 0x123456);
        assert_eq!(source.colors.unwrap()[7], st::LIGHT[7]);
        assert!(read_source(path, Some(source.stamp), false)
            .unwrap()
            .colors
            .is_none());
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn desktop_palette_uses_absolute_xdg_cache_with_home_fallback() {
        assert_eq!(
            palette_path(Some("/cache".into()), Some("/home/test".into())),
            Some("/cache/ryoku/colors.json".into())
        );
        assert_eq!(
            palette_path(Some("relative".into()), Some("/home/test".into())),
            Some("/home/test/.cache/ryoku/colors.json".into())
        );
        assert_eq!(palette_path(None, None), None);
    }
}
