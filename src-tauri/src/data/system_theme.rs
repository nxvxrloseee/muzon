//! Colours from the desktop shell.
//!
//! Shells that generate a Material You palette from the wallpaper write it to a
//! JSON file: caelestia and rice both use `{"colours": {"primary": "9bd0cc", …}}`
//! under `$XDG_STATE_HOME`. When the user picks the system theme we read that
//! file and follow it, so recolouring the desktop recolours Muzon too.
//!
//! The file is polled rather than watched: one `stat` every two seconds costs
//! nothing and saves a dependency on an inotify crate.

use crate::domain::Theme;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

const POLL: Duration = Duration::from_secs(2);

#[derive(serde::Deserialize)]
struct Scheme {
    #[serde(alias = "colors")]
    colours: HashMap<String, String>,
}

/// Where the shell's palette lives. `MUZON_SCHEME_FILE` wins, so anyone with a
/// different shell can point Muzon at their own file.
pub fn scheme_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("MUZON_SCHEME_FILE") {
        let path = PathBuf::from(path);
        return path.exists().then_some(path);
    }

    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs_home().map(|h| h.join(".local/state")))?;

    ["caelestia/scheme.json", "rice/scheme.json"]
        .iter()
        .map(|rel| state.join(rel))
        .find(|p| p.exists())
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Read the shell's palette and translate it into Muzon's roles.
pub fn load() -> anyhow::Result<Theme> {
    let path = scheme_path().ok_or_else(|| anyhow::anyhow!("палитра системы не найдена"))?;
    load_from(&path)
}

pub fn load_from(path: &std::path::Path) -> anyhow::Result<Theme> {
    let text = std::fs::read_to_string(path)?;
    let scheme: Scheme = serde_json::from_str(&text)?;
    Ok(Theme::from_material(&scheme.colours))
}

fn modified(path: &std::path::Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

/// Follow the palette file until `stop` says otherwise, calling `on_change`
/// with every new theme. Runs on its own thread.
pub fn watch<F, S>(on_change: F, stop: S)
where
    F: Fn(Theme) + Send + 'static,
    S: Fn() -> bool + Send + 'static,
{
    std::thread::spawn(move || {
        let mut last = scheme_path().as_deref().and_then(modified);

        while !stop() {
            std::thread::sleep(POLL);

            let Some(path) = scheme_path() else {
                continue;
            };
            let Some(stamp) = modified(&path) else {
                continue;
            };
            if Some(stamp) == last {
                continue;
            }
            last = Some(stamp);

            match load_from(&path) {
                Ok(theme) => on_change(theme),
                Err(e) => eprintln!("[muzon theme] не удалось прочитать палитру системы: {e}"),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write(dir: &std::path::Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(body.as_bytes()).unwrap();
        path
    }

    #[test]
    fn reads_a_shell_scheme() {
        let dir = std::env::temp_dir().join(format!("muzon-theme-{}", std::process::id()));
        let path = write(
            &dir,
            "scheme.json",
            r#"{"name":"caelestia","colours":{"surface":"0a0f0f","primary":"9bd0cc","onSurface":"dce8e6"}}"#,
        );

        let theme = load_from(&path).unwrap();
        assert_eq!(theme.background, "#0a0f0f");
        assert_eq!(theme.accent_primary, "#9bd0cc");
        assert!(theme.validate().is_ok());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_a_file_that_is_not_a_scheme() {
        let dir = std::env::temp_dir().join(format!("muzon-theme-bad-{}", std::process::id()));
        let path = write(&dir, "scheme.json", "not json at all");
        assert!(load_from(&path).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
