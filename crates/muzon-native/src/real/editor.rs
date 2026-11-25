//! Drafts stay local until explicit Save. Async answers carry an editor generation.
use super::{message::Message, state::RealApp, style as st};
use iced::{
    widget::{button, checkbox, column, container, row, text, text_editor, text_input},
    Element, Length,
};
use muzon_core::{
    domain::library::TrackEditInput,
    editing::{EditRequest, EditorSource},
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Tags,
    Lyrics,
}
pub struct Editor {
    pub id: i32,
    pub generation: u64,
    pub source: EditorSource,
    pub fields: [String; 7],
    pub lyrics: text_editor::Content,
    pub tab: Tab,
    pub in_tag: bool,
    pub loading: bool,
    pub saving: bool,
    pub tags_dirty: bool,
    pub lyrics_dirty: bool,
    pub discard: bool,
    pub error: Option<String>,
    pub fetch: Option<iced::task::Handle>,
    pub fetch_serial: u64,
    pub suggestion: Option<muzon_core::data::lrclib::Suggestion>,
    pub tap: Option<TapDraft>,
}
impl Editor {
    pub fn new(generation: u64, source: EditorSource) -> Self {
        let in_tag = source.lyrics_in_tag;
        let t = &source.track;
        let fields = [
            t.title.clone(),
            t.artist.clone().unwrap_or_default(),
            t.album.clone().unwrap_or_default(),
            t.track_no.map(|v| v.to_string()).unwrap_or_default(),
            t.genre.clone().unwrap_or_default(),
            t.year.map(|v| v.to_string()).unwrap_or_default(),
            String::new(),
        ];
        Self {
            id: t.id,
            generation,
            fields,
            lyrics: text_editor::Content::with_text(&source.lyrics),
            source,
            tab: Tab::Tags,
            in_tag,
            loading: true,
            saving: false,
            tags_dirty: false,
            lyrics_dirty: false,
            discard: false,
            error: None,
            fetch: None,
            fetch_serial: 0,
            suggestion: None,
            tap: None,
        }
    }
    pub fn dirty(&self) -> bool {
        self.tags_dirty
            || self.lyrics_dirty
            || self.tap.as_ref().is_some_and(|tap| !tap.stamps.is_empty())
    }
    pub fn request(&self) -> Result<EditRequest, String> {
        if self.tab == Tab::Lyrics {
            return Ok(EditRequest::Lyrics {
                text: self.lyrics.text(),
                in_tag: self.in_tag,
            });
        }
        let optional = |s: &String| (!s.trim().is_empty()).then(|| s.trim().to_owned());
        let number = |s: &String, max: i32| -> Result<Option<i32>, String> {
            if s.trim().is_empty() {
                return Ok(None);
            }
            let n = s
                .trim()
                .parse::<i32>()
                .map_err(|_| "Номер трека и год должны быть целыми числами".to_owned())?;
            if n <= 0 || n > max {
                return Err("Некорректный номер трека или год".into());
            }
            Ok(Some(n))
        };
        if self.fields[0].trim().is_empty() {
            return Err("Укажите название трека".into());
        }
        Ok(EditRequest::Tags(TrackEditInput {
            title: self.fields[0].trim().into(),
            artist: optional(&self.fields[1]),
            album: optional(&self.fields[2]),
            track_no: number(&self.fields[3], i32::MAX)?,
            genre: optional(&self.fields[4]),
            year: number(&self.fields[5], 9999)?,
            cover_path: optional(&self.fields[6]),
        }))
    }
}
pub fn overlay(app: &RealApp) -> Option<Element<'_, Message>> {
    let editor = app.editor.as_ref()?;
    let enabled = !editor.loading && !editor.saving;
    let mut panel = column![
        row![
            text("Редактор трека")
                .size(st::type_size::TITLE)
                .width(Length::Fill),
            button("Закрыть")
                .style(st::quiet)
                .on_press(Message::EditorClose)
        ]
        .spacing(st::space::MD),
        text(&editor.source.track.path)
            .size(st::type_size::CAPTION)
            .color(st::muted()),
        row![
            button("Теги")
                .style(if editor.tab == Tab::Tags {
                    st::primary
                } else {
                    st::secondary
                })
                .padding([8, 14])
                .on_press_maybe(enabled.then_some(Message::EditorTab(Tab::Tags))),
            button("Текст / LRC")
                .style(if editor.tab == Tab::Lyrics {
                    st::primary
                } else {
                    st::secondary
                })
                .padding([8, 14])
                .on_press_maybe(enabled.then_some(Message::EditorTab(Tab::Lyrics)))
        ]
        .spacing(10)
    ]
    .spacing(14);
    if editor.loading {
        panel = panel.push(text("Чтение файла…").color(st::muted()));
    } else if editor.tab == Tab::Tags {
        let mut fields = column![].spacing(10);
        for (i, label) in [
            "Название",
            "Исполнитель",
            "Альбом",
            "Номер трека",
            "Жанр",
            "Год",
            "Путь к обложке PNG/JPEG (пусто — сохранить прежнюю)",
        ]
        .into_iter()
        .enumerate()
        {
            fields = fields.push(
                column![
                    text(label).size(st::type_size::CAPTION).color(st::muted()),
                    text_input(label, &editor.fields[i])
                        .padding(st::space::SM)
                        .size(st::type_size::BODY)
                        .style(st::input)
                        .on_input_maybe(
                            enabled.then_some(move |value| Message::EditorField(i, value))
                        )
                ]
                .spacing(st::space::XS),
            );
        }
        panel = panel.push(
            st::scrollable(fields)
                .height(Length::Fill)
                .style(st::scroll),
        );
    } else {
        let same = app.snapshot.current_path.as_deref() == Some(editor.source.track.path.as_str());
        panel=panel.push(row![
            button(if editor.fetch.is_some(){"Отменить поиск"}else{"Найти в LRCLIB"}).padding([8,12]).style(st::secondary).on_press_maybe(enabled.then_some(if editor.fetch.is_some(){Message::EditorFetchCancel}else{Message::EditorFetch})),
            button("Простучать строки").padding([8,12]).style(st::quiet).on_press_maybe((enabled&&same&&editor.tap.is_none()).then_some(Message::EditorTapStart)),
            button(if app.snapshot.is_playing{"Пауза"}else{"Продолжить"}).padding([8,12]).style(st::quiet).on_press_maybe(same.then_some(Message::TogglePlay))
        ].spacing(st::space::SM)).push(text("Поиск отправляет в LRCLIB название, исполнителя, альбом и длительность из редактора.").size(st::type_size::CAPTION).color(st::muted()));
        if let Some(suggestion) = &editor.suggestion {
            let preview: String = suggestion
                .text
                .lines()
                .take(8)
                .collect::<Vec<_>>()
                .join("\n")
                .chars()
                .take(2000)
                .collect();
            panel = panel.push(
                container(
                    column![
                        text(if suggestion.synced {
                            "Найден синхронный LRC"
                        } else {
                            "Найден текст без временных меток"
                        })
                        .size(st::type_size::BODY),
                        text(preview)
                            .size(st::type_size::CAPTION)
                            .color(st::muted()),
                        row![
                            button("Заменить черновик найденным текстом")
                                .style(st::secondary)
                                .padding(st::space::SM)
                                .on_press_maybe(
                                    (enabled && editor.tap.is_none())
                                        .then_some(Message::EditorUseSuggestion)
                                ),
                            button("Убрать результат")
                                .style(st::quiet)
                                .padding(st::space::SM)
                                .on_press(Message::EditorDismissSuggestion)
                        ]
                        .spacing(st::space::SM)
                    ]
                    .spacing(st::space::SM),
                )
                .padding(st::space::MD)
                .style(st::card),
            );
        }
        if let Some(tap) = &editor.tap {
            let line = tap
                .lines
                .get(tap.stamps.len())
                .map(|s| s.chars().take(200).collect::<String>())
                .unwrap_or_else(|| "Все строки отмечены".into());
            panel = panel.push(
                column![
                    text(format!(
                        "Отмечено {} из {}",
                        tap.stamps.len(),
                        tap.lines.len()
                    ))
                    .size(st::type_size::BODY)
                    .color(st::accent()),
                    text(line).size(st::type_size::EMPHASIS),
                    row![
                        button("Отметить (пробел)")
                            .style(st::primary)
                            .padding(10)
                            .on_press_maybe(
                                (same && tap.stamps.len() < tap.lines.len())
                                    .then_some(Message::EditorTapStamp)
                            ),
                        button("Назад")
                            .style(st::secondary)
                            .padding(10)
                            .on_press_maybe(
                                (!tap.stamps.is_empty()).then_some(Message::EditorTapUndo)
                            ),
                        button("Готово")
                            .style(st::secondary)
                            .padding(10)
                            .on_press_maybe(
                                (!tap.stamps.is_empty()).then_some(Message::EditorTapFinish)
                            ),
                        button("Отменить простукивание")
                            .style(st::quiet)
                            .padding(10)
                            .on_press(Message::EditorTapCancel)
                    ]
                    .spacing(st::space::SM)
                ]
                .spacing(14),
            );
        } else {
            panel = panel
                .push(
                    row![
                        button("Вставить текущее время")
                            .padding([8, 12])
                            .style(st::quiet)
                            .on_press_maybe((enabled && same).then_some(Message::EditorTimestamp)),
                        text("Формат: [00:12.34]Текст")
                            .size(st::type_size::CAPTION)
                            .color(st::muted())
                    ]
                    .spacing(st::space::MD),
                )
                .push(
                    text_editor(&editor.lyrics)
                        .style(st::editor)
                        .padding(st::space::MD)
                        .height(Length::Fill)
                        .on_action(Message::EditorText),
                );
        }
        panel = panel.push(
            checkbox("Сохранить внутри аудиофайла", editor.in_tag)
                .on_toggle(Message::EditorStorage),
        );
    }

    if let Some(error) = &editor.error {
        panel = panel.push(
            text(error)
                .size(st::type_size::CAPTION)
                .color(iced::color!(0xff9696)),
        );
    }
    if editor.discard {
        panel = panel
            .push(text("Есть несохранённые изменения. Отбросить их?").size(st::type_size::BODY))
            .push(
                row![
                    button("Продолжить редактирование")
                        .style(st::secondary)
                        .padding(10)
                        .on_press(Message::EditorKeep),
                    button("Отбросить изменения")
                        .style(st::quiet)
                        .padding(10)
                        .on_press(Message::EditorDiscard)
                ]
                .spacing(st::space::MD),
            );
    } else {
        let dirty = if editor.tab == Tab::Tags {
            editor.tags_dirty
        } else {
            editor.lyrics_dirty
        };
        panel = panel.push(
            row![
                text(if editor.saving {
                    "Запись…"
                } else if editor.dirty() {
                    "Есть несохранённые изменения"
                } else {
                    "Изменения сохранены"
                })
                .size(st::type_size::CAPTION)
                .color(st::muted())
                .width(Length::Fill),
                button("Сохранить")
                    .padding([10, 18])
                    .style(st::primary)
                    .on_press_maybe(
                        (enabled && dirty && editor.tap.is_none()).then_some(Message::EditorSave)
                    )
            ]
            .spacing(st::space::MD),
        );
    }
    let body = container(panel.height(Length::Fill))
        .padding(22)
        .max_width(720)
        .height(Length::Fill)
        .style(st::card);
    Some(
        container(body)
            .padding(st::space::XL)
            .center(Length::Fill)
            .style(|_| st::surface(iced::Color::from_rgba(0., 0., 0., 0.78)))
            .into(),
    )
}

pub fn timestamp(seconds: f64) -> String {
    let cents = if seconds.is_finite() {
        (seconds.max(0.0) * 100.0).round() as u64
    } else {
        0
    };
    format!(
        "[{:02}:{:02}.{:02}]",
        cents / 6000,
        (cents / 100) % 60,
        cents % 100
    )
}
pub struct TapDraft {
    pub lines: Vec<String>,
    pub stamps: Vec<f64>,
}
impl TapDraft {
    pub fn new(text: &str) -> Result<Self, String> {
        let lines: Vec<String> = text
            .lines()
            .map(strip_timestamp)
            .filter(|s| !s.is_empty())
            .collect();
        if lines.is_empty() {
            return Err("Сначала введите или загрузите текст".into());
        }
        if lines.len() > 10000 {
            return Err("Для простукивания допускается не больше 10000 строк".into());
        }
        Ok(Self {
            lines,
            stamps: Vec::new(),
        })
    }
    pub fn stamp(&mut self, position: f64) -> Result<(), String> {
        if !position.is_finite() || position < 0.0 {
            return Err("Нет позиции воспроизведения".into());
        }
        if self.stamps.last().is_some_and(|last| position < *last) {
            return Err("Время должно идти вперёд. Удалите последние метки кнопкой «Назад»".into());
        }
        if self.stamps.len() < self.lines.len() {
            self.stamps.push(position);
        }
        Ok(())
    }
    pub fn text(&self) -> String {
        self.lines
            .iter()
            .enumerate()
            .map(|(i, line)| {
                self.stamps
                    .get(i)
                    .map(|position| format!("{}{line}", timestamp(*position)))
                    .unwrap_or_else(|| line.clone())
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
fn strip_timestamp(line: &str) -> String {
    let mut line = line.trim();
    while let Some(rest) = line.strip_prefix('[') {
        let Some(end) = rest.find(']') else {
            break;
        };
        let token = &rest[..end];
        if !token.contains(':')
            || !token
                .chars()
                .all(|c| c.is_ascii_digit() || matches!(c, ':' | '.'))
        {
            break;
        }
        line = rest[end + 1..].trim_start();
    }
    if ["[ar:", "[ti:", "[al:", "[by:", "[offset:"]
        .iter()
        .any(|tag| line.starts_with(tag))
    {
        String::new()
    } else {
        line.trim().to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tap_draft_keeps_unstamped_lines_and_rejects_backward_times() {
        let mut tap = TapDraft::new("[ar:artist]\n[00:01.00]one\ntwo\nthree").unwrap();
        assert_eq!(tap.lines, vec!["one", "two", "three"]);
        tap.stamp(2.).unwrap();
        tap.stamp(3.).unwrap();
        assert!(tap.stamp(1.).is_err());
        assert_eq!(tap.text(), "[00:02.00]one\n[00:03.00]two\nthree");
        tap.stamps.pop();
        assert_eq!(tap.stamps.len(), 1);
    }
    #[test]
    fn timestamp_rounding_carries_to_next_minute() {
        assert_eq!(timestamp(59.999), "[01:00.00]");
        assert_eq!(timestamp(-5.0), "[00:00.00]");
        assert_eq!(timestamp(f64::NAN), "[00:00.00]");
    }
}
