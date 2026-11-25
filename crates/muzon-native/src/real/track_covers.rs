//! One bounded thumbnail batch, shared by every virtualized track list.
use super::{
    icons::{icon, Icon},
    message::Message,
    state::{RealApp, Screen},
    style as st,
};
use iced::{
    widget::{container, image},
    Element, Length, Task,
};
use std::{collections::HashMap, path::PathBuf};
const LIMIT: usize = 128;
pub type Loaded = Vec<(String, Option<Vec<u8>>, Option<String>)>;
pub struct Covers {
    cache: HashMap<String, Option<image::Handle>>,
    formats: HashMap<String, String>,
    directory: PathBuf,
    epoch: u64,
    busy: bool,
}
impl Covers {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            cache: HashMap::new(),
            formats: HashMap::new(),
            directory,
            epoch: 0,
            busy: false,
        }
    }
    pub fn invalidate(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        self.cache.clear();
        self.formats.clear();
    }
    pub fn finish(&mut self, epoch: u64, loaded: Loaded) {
        self.busy = false;
        if epoch == self.epoch {
            for (path, bytes, format) in loaded {
                if let Some(format) = format {
                    self.formats.insert(path.clone(), format);
                }
                self.cache
                    .insert(path, bytes.map(image::Handle::from_bytes));
            }
        }
    }
    /// Actual stream properties, read off the UI thread with the thumbnail batch.
    pub fn format(&self, path: &str) -> Option<&str> {
        self.formats.get(path).map(String::as_str)
    }
    pub fn view<'a>(&self, path: &str) -> Element<'a, Message> {
        let art: Element<'a, Message> = match self.cache.get(path) {
            Some(Some(handle)) => image(handle.clone())
                .width(32.)
                .height(32.)
                .content_fit(iced::ContentFit::Contain)
                .into(),
            _ => container(icon(Icon::Music, 18., st::muted()))
                .center(Length::Fill)
                .style(st::card)
                .into(),
        };
        container(art).width(32.).height(32.).clip(true).into()
    }
}
pub fn has_rows(app: &RealApp) -> bool {
    matches!(
        app.screen,
        Screen::Library
            | Screen::Favorites
            | Screen::Smart
            | Screen::Playlists
            | Screen::Artists
            | Screen::Genres
    ) || app.screen == Screen::Albums && !app.album_grid.browsing
}
pub fn load(app: &mut RealApp) -> Task<Message> {
    if !has_rows(app) || app.hidden || app.track_covers.busy {
        return Task::none();
    }
    let visible: Vec<_> = app.filtered
        [crate::ui::visible_window(app.library_scroll, app.filtered.len())]
    .iter()
    .filter_map(|&i| app.snapshot.library.get(i).map(|t| t.path.clone()))
    .collect();
    if app.track_covers.cache.len() + 8 > LIMIT {
        app.track_covers
            .cache
            .retain(|path, _| visible.contains(path));
    }
    app.track_covers
        .formats
        .retain(|path, _| app.track_covers.cache.contains_key(path));
    let jobs: Vec<_> = visible
        .into_iter()
        .filter(|p| !app.track_covers.cache.contains_key(p))
        .take(8)
        .collect();
    if jobs.is_empty() {
        return Task::none();
    }
    app.track_covers.busy = true;
    let failed: Loaded = jobs.iter().map(|p| (p.clone(), None, None)).collect();
    let directory = app.track_covers.directory.clone();
    let epoch = app.track_covers.epoch;
    Task::perform(
        async move {
            let loaded = tokio::task::spawn_blocking(move || {
                jobs.into_iter()
                    .map(|path| {
                        let bytes = muzon_core::data::covers::read_cover_thumbnail(
                            &directory,
                            std::path::Path::new(&path),
                        )
                        .map(|(_, b)| b)
                        .filter(|b| b.len() <= 2 * 1024 * 1024);
                        let format =
                            muzon_core::data::tags::read_audio_format(std::path::Path::new(&path))
                                .ok();
                        (path, bytes, format)
                    })
                    .collect::<Loaded>()
            })
            .await
            .unwrap_or(failed);
            (epoch, loaded)
        },
        |(epoch, loaded)| Message::TrackCovers(epoch, loaded),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_generation_does_not_replace_new_art() {
        let mut cache = Covers::new(PathBuf::new());
        cache.busy = true;
        cache.invalidate();
        cache.finish(0, vec![("/old".into(), None, None)]);
        assert!(!cache.busy);
        assert!(cache.cache.is_empty());
    }
}
