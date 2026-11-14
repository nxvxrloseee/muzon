//! Profile-local key bindings with ordered, atomic background saves.
use super::{message::Message, shortcuts::Shortcut, state::RealApp, style as st};
use iced::{
    keyboard::{key::Named, Key, Modifiers},
    widget::{button, column, row, scrollable, text},
    Element, Length,
};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
const NAMES: [&str; 8] = [
    "playPause",
    "seekForward",
    "seekBackward",
    "volumeUp",
    "volumeDown",
    "nextTrack",
    "previousTrack",
    "closeNowPlaying",
];
const LABELS: [&str; 8] = [
    "Пауза / воспроизведение",
    "Вперёд на 5 секунд",
    "Назад на 5 секунд",
    "Громче",
    "Тише",
    "Следующий трек",
    "Предыдущий трек",
    "Вернуться к библиотеке",
];
const DEFAULT: [&str; 8] = [
    "Space",
    "ArrowRight",
    "ArrowLeft",
    "ArrowUp",
    "ArrowDown",
    "n",
    "p",
    "Escape",
];
const ACTIONS: [Shortcut; 8] = [
    Shortcut::Toggle,
    Shortcut::SeekForward,
    Shortcut::SeekBack,
    Shortcut::VolumeUp,
    Shortcut::VolumeDown,
    Shortcut::Next,
    Shortcut::Previous,
    Shortcut::Escape,
];
#[derive(Clone)]
pub struct Store {
    path: PathBuf,
    written: Arc<Mutex<u64>>,
}
impl Store {
    pub fn save(&self, revision: u64, values: &[String; 8]) -> Result<(), String> {
        let mut written = self.written.lock().map_err(|e| e.to_string())?;
        if revision <= *written {
            return Ok(());
        }
        let result = (|| -> anyhow::Result<()> {
            std::fs::create_dir_all(self.path.parent().unwrap())?;
            let mut object = serde_json::Map::new();
            for (name, value) in NAMES.iter().zip(values) {
                object.insert(
                    (*name).into(),
                    serde_json::json!(if value == "Space" { " " } else { value }),
                );
            }
            use std::io::Write;
            let temporary = self.path.with_extension("json.tmp");
            let write = (|| -> anyhow::Result<()> {
                let mut file = std::fs::File::create(&temporary)?;
                file.write_all(&serde_json::to_vec_pretty(&object)?)?;
                file.sync_all()?;
                std::fs::rename(&temporary, &self.path)?;
                Ok(())
            })();
            if write.is_err() {
                let _ = std::fs::remove_file(&temporary);
            }
            write
        })();
        result.map_err(|e| e.to_string())?;
        *written = revision;
        Ok(())
    }
}
pub struct Hotkeys {
    pub values: [String; 8],
    pub capture: Option<usize>,
    pub error: Option<String>,
    pub dirty: bool,
    revision: u64,
    store: Store,
}
impl Hotkeys {
    pub fn new(profile: &Path) -> Self {
        let store = Store {
            path: profile.join("config/hotkeys.json"),
            written: Arc::new(Mutex::new(0)),
        };
        let defaults = DEFAULT.map(str::to_owned);
        let values = std::fs::read(&store.path)
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
            .and_then(|v| {
                let mut values = defaults.clone();
                for (i, name) in NAMES.iter().enumerate() {
                    if let Some(value) = v.get(name).and_then(|v| v.as_str()) {
                        values[i] = if value == " " {
                            "Space".into()
                        } else if value.chars().count() == 1 {
                            value.to_lowercase()
                        } else {
                            value.into()
                        };
                    }
                }
                valid(&values).then_some(values)
            })
            .unwrap_or(defaults);
        Self {
            values,
            capture: None,
            error: None,
            dirty: false,
            revision: 0,
            store,
        }
    }
    pub fn resolve(&self, key: &Key, modifiers: Modifiers) -> Option<Shortcut> {
        if modifiers.control() && !modifiers.alt() && !modifiers.logo() {
            if matches!(key,Key::Character(c)if c.eq_ignore_ascii_case("f")) {
                return Some(Shortcut::Search);
            }
            if matches!(key,Key::Character(c)if c.eq_ignore_ascii_case("k")) {
                return Some(Shortcut::Palette);
            }
        }
        if modifiers.control() || modifiers.alt() || modifiers.logo() || modifiers.shift() {
            return None;
        }
        if *key == Key::Named(Named::Escape) {
            return Some(Shortcut::Escape);
        }
        let canonical = canonical(key)?;
        self.values
            .iter()
            .position(|value| value == &canonical)
            .map(|i| ACTIONS[i])
    }
    pub fn bind(&mut self, index: usize, key: &Key, modifiers: Modifiers) -> Result<(), String> {
        if index >= 8 {
            return Err("Неизвестное действие".into());
        }
        if modifiers.control() || modifiers.alt() || modifiers.logo() || modifiers.shift() {
            return Err("Нажмите клавишу без Ctrl, Alt, Shift или Super".into());
        }
        let key = canonical(key).ok_or("Эта клавиша не поддерживается")?;
        let mut values = self.values.clone();
        values[index] = key;
        if !valid(&values) {
            return Err("Клавиша уже занята другим действием".into());
        }
        self.values = values;
        self.revision += 1;
        self.dirty = true;
        self.capture = None;
        self.error = None;
        Ok(())
    }
    pub fn reset(&mut self) {
        self.values = DEFAULT.map(str::to_owned);
        self.revision += 1;
        self.dirty = true;
        self.capture = None;
        self.error = None;
    }
    pub fn flush(&self) -> (Store, u64, [String; 8]) {
        (self.store.clone(), self.revision, self.values.clone())
    }
    pub fn saved(&mut self, revision: u64, result: Result<(), String>) {
        if revision == self.revision {
            match result {
                Ok(()) => {
                    self.dirty = false;
                    self.error = None;
                }
                Err(error) => self.error = Some(error),
            }
        }
    }
}
fn valid(values: &[String; 8]) -> bool {
    !values[..7].iter().any(|v| v == "Escape")
        && values.iter().all(|v| !v.is_empty() && v.len() <= 64)
        && values
            .iter()
            .enumerate()
            .all(|(i, v)| !values[..i].contains(v))
}
fn canonical(key: &Key) -> Option<String> {
    match key {
        Key::Character(c) if c.chars().count() == 1 => Some(if c == " " {
            "Space".into()
        } else {
            c.to_lowercase()
        }),
        Key::Named(named)
            if !matches!(
                named,
                Named::Control | Named::Alt | Named::Shift | Named::Super
            ) =>
        {
            Some(format!("{named:?}"))
        }
        _ => None,
    }
}
pub fn view(app: &RealApp) -> Element<'_, Message> {
    let hotkeys = &app.hotkeys;
    let mut content = column![
        text("Горячие клавиши").size(28),
        text("Нажмите на привязку, затем одну клавишу. Escape отменяет назначение.")
            .size(13)
            .color(st::muted())
    ]
    .spacing(16);
    for (i, label) in LABELS.into_iter().enumerate() {
        content = content.push(
            row![
                text(label).size(14).width(Length::Fill),
                button(
                    text(if hotkeys.capture == Some(i) {
                        "Нажмите клавишу…"
                    } else if hotkeys.values[i] == "Space" {
                        "Пробел"
                    } else {
                        &hotkeys.values[i]
                    })
                    .size(13)
                )
                .width(190)
                .padding(10)
                .style(if hotkeys.capture == Some(i) {
                    st::primary
                } else {
                    st::secondary
                })
                .on_press(Message::HotkeyCapture(i))
            ]
            .spacing(16),
        );
    }
    if let Some(error) = &hotkeys.error {
        content = content.push(text(error).size(13).color(iced::color!(0xff9696)));
    }
    content=content.push(row![button("Сбросить привязки").padding(10).style(st::secondary).on_press(Message::HotkeysReset),button("Сохранить").padding(10).style(st::quiet).on_press_maybe(hotkeys.dirty.then_some(Message::HotkeysSave))].spacing(12))
        .push(text("Ctrl+F — поиск, Ctrl+K — палитра команд. Эти сочетания и Escape зарезервированы. Привязки действуют внутри окна; ввод текста и ползунки имеют приоритет.").size(12).color(st::muted()));
    scrollable(content)
        .style(st::scroll)
        .height(Length::Fill)
        .into()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rebinding_rejects_duplicates_and_persists() {
        let dir = muzon_core::testing::scratch_dir("native-hotkeys");
        let mut keys = Hotkeys::new(&dir);
        assert!(keys
            .bind(0, &Key::Character("n".into()), Modifiers::empty())
            .is_err());
        keys.bind(0, &Key::Character("x".into()), Modifiers::empty())
            .unwrap();
        let (store, rev, values) = keys.flush();
        store.save(rev, &values).unwrap();
        let loaded = Hotkeys::new(&dir);
        assert_eq!(
            loaded.resolve(&Key::Character("x".into()), Modifiers::empty()),
            Some(Shortcut::Toggle)
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
