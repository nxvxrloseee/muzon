//! Desktop palettes: an explicit override, Ryoku's cache, then legacy shells.
use crate::domain::Theme;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

const POLL: Duration = Duration::from_secs(2);

fn xdg(name: &str, fallback: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(fallback)))
}

fn candidates() -> Vec<PathBuf> {
    if let Some(path) = std::env::var_os("MUZON_SCHEME_FILE").filter(|p| !p.is_empty()) {
        return vec![PathBuf::from(path)];
    }
    let mut paths = Vec::new();
    if let Some(cache) = xdg("XDG_CACHE_HOME", ".cache") {
        paths.push(cache.join("ryoku/colors.json"));
    }
    if let Some(state) = xdg("XDG_STATE_HOME", ".local/state") {
        paths.push(state.join("caelestia/scheme.json"));
        paths.push(state.join("rice/scheme.json"));
    }
    paths
}

/// The first available source, with Ryoku ahead of leftover legacy files.
/// An explicit override remains selected even before its file appears.
pub fn scheme_path() -> Option<PathBuf> {
    select_path(&candidates())
}
fn select_path(paths: &[PathBuf]) -> Option<PathBuf> {
    paths
        .iter()
        .find(|p| p.is_file())
        .cloned()
        .or_else(|| paths.first().cloned())
}

pub fn load() -> anyhow::Result<Theme> {
    let path = scheme_path().ok_or_else(|| anyhow::anyhow!("палитра системы пока недоступна"))?;
    load_from(&path)
}

fn parse(text: &str) -> anyhow::Result<Theme> {
    let value: serde_json::Value = serde_json::from_str(text)?;
    let object = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("ожидается карта цветов"))?;
    let colours = object
        .get("colours")
        .or_else(|| object.get("colors"))
        .unwrap_or(&value);
    let map: HashMap<String, String> = serde_json::from_value(colours.clone())?;
    // A random object (or a terminal-only palette) must not look like success.
    let valid = |key: &str| {
        map.get(key)
            .is_some_and(|v| crate::domain::theme::is_valid_hex_color(v.trim()))
    };
    anyhow::ensure!(
        ["surface", "background"].iter().any(|k| valid(k))
            && valid("primary")
            && ["onSurface", "onBackground"].iter().any(|k| valid(k)),
        "неполная палитра: нужны фон, primary и цвет текста"
    );
    for (key, color) in &map {
        anyhow::ensure!(
            crate::domain::theme::is_valid_hex_color(color.trim()),
            "неверный цвет роли {key}"
        );
    }
    let theme = Theme::from_material(&map);
    theme.validate().map_err(anyhow::Error::msg)?;
    Ok(theme)
}
pub fn load_from(path: &Path) -> anyhow::Result<Theme> {
    parse(&std::fs::read_to_string(path)?)
}

/// Compare the source path AND bytes, not just mtime: atomic replacement,
/// equal timestamps across sources and short-lived invalid JSON are common.
#[derive(Default)]
struct PaletteWatch {
    accepted: Option<(PathBuf, String)>,
}
impl PaletteWatch {
    fn poll(&mut self, paths: &[PathBuf]) -> Option<Theme> {
        let Some(path) = select_path(paths) else {
            self.accepted = None;
            return None;
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            self.accepted = None;
            return None;
        };
        if self.accepted.as_ref() == Some(&(path.clone(), text.clone())) {
            return None;
        }
        let theme = parse(&text).ok()?;
        self.accepted = Some((path, text));
        Some(theme)
    }
}

pub fn watch<F, S>(on_change: F, stop: S)
where
    F: Fn(Theme) + Send + 'static,
    S: Fn() -> bool + Send + 'static,
{
    std::thread::spawn(move || {
        let mut watcher = PaletteWatch::default();
        while !stop() {
            if let Some(theme) = watcher.poll(&candidates()) {
                on_change(theme);
            }
            std::thread::sleep(POLL);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    const FLAT: &str = r##"{"background":"#fafafa","onSurface":"#141414","primary":"#444444"}"##;
    #[test]
    fn formats_and_validation() {
        for text in [
            FLAT.to_string(),
            format!("{{\"colours\":{FLAT}}}"),
            format!("{{\"colors\":{FLAT}}}"),
            FLAT.replace('#', ""),
        ] {
            let theme = parse(&text).unwrap();
            assert_eq!(theme.background, "#fafafa");
            assert_eq!(theme.text_primary, "#141414");
            assert_eq!(
                theme.card_background,
                Theme::default_light().card_background
            );
        }
        for text in [
            "",
            "{}",
            "[]",
            "null",
            "broken",
            r#"{"name":"other"}"#,
            r#"{"primary":"bad"}"#,
        ] {
            assert!(parse(text).is_err(), "{text}");
        }
        assert!(parse(&FLAT.replace("#444444", "bad")).is_err());
    }
    #[test]
    fn appearance_replace_source_switch_and_recovery() {
        let dir = crate::testing::scratch_dir("palette-watch");
        let ryoku = dir.join("colors.json");
        let legacy = dir.join("scheme.json");
        let paths = [ryoku.clone(), legacy.clone()];
        let mut watcher = PaletteWatch::default();
        assert!(watcher.poll(&paths).is_none());
        std::fs::write(&legacy, FLAT).unwrap();
        assert!(watcher.poll(&paths).is_some());
        assert!(watcher.poll(&paths).is_none());
        std::fs::write(&ryoku, FLAT).unwrap();
        assert!(watcher.poll(&paths).is_some());
        std::fs::write(&ryoku, "broken").unwrap();
        assert!(watcher.poll(&paths).is_none());
        let replacement = dir.join("tmp");
        std::fs::write(&replacement, FLAT).unwrap();
        std::fs::rename(replacement, &ryoku).unwrap();
        // Same good contents after a transient bad write need no repaint.
        assert!(watcher.poll(&paths).is_none());
        std::fs::remove_file(&ryoku).unwrap();
        assert!(watcher.poll(&paths).is_some());
        std::fs::remove_file(&legacy).unwrap();
        assert!(watcher.poll(&paths).is_none());
        std::fs::write(&legacy, FLAT).unwrap();
        assert!(watcher.poll(&paths).is_some());
    }
}
