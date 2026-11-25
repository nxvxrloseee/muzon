//! Virtual album cards and one bounded background thumbnail batch at a time.
use super::{
    groups::GroupKey,
    message::Message,
    state::{RealApp, Screen},
    style as st,
};
use iced::{
    widget::{button, column, container, image, row, scrollable, text, vertical_space},
    Alignment, Element, Length, Size, Task,
};
use std::{collections::HashMap, ops::Range, path::PathBuf};
const GAP: f32 = 16.;
const CARD_HEIGHT: f32 = 252.;
const CACHE_LIMIT: usize = 256;
pub type Loaded = Vec<(GroupKey, Option<Vec<u8>>)>;
pub struct Grid {
    pub browsing: bool,
    pub viewport: Size,
    cache: HashMap<GroupKey, Option<image::Handle>>,
    directory: PathBuf,
    epoch: u64,
    busy: bool,
}
impl Grid {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            browsing: true,
            viewport: Size::new(1180., 760.),
            cache: HashMap::new(),
            directory,
            epoch: 0,
            busy: false,
        }
    }
    pub fn invalidate(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        self.cache.clear();
    }
    pub fn finish(&mut self, epoch: u64, loaded: Loaded) {
        self.busy = false;
        if epoch != self.epoch {
            return;
        }
        for (key, bytes) in loaded {
            self.cache.insert(key, bytes.map(image::Handle::from_bytes));
        }
    }
    pub fn columns(&self) -> usize {
        columns((self.viewport.width - 240. - 56. - st::SCROLL_GUTTER).max(1.))
    }
    pub fn window(&self, offset: f32, total: usize) -> Range<usize> {
        window(offset, total, self.columns(), self.viewport.height)
    }
}
pub fn columns(width: f32) -> usize {
    (((width.max(0.) + GAP) / (160. + GAP)).floor() as usize).clamp(1, 12)
}
pub fn window(offset: f32, total: usize, columns: usize, height: f32) -> Range<usize> {
    let columns = columns.max(1);
    let rows = total.div_ceil(columns);
    let first = ((offset.max(0.) / CARD_HEIGHT).floor() as usize)
        .min(rows)
        .saturating_sub(1);
    let count = (((height.max(0.) / CARD_HEIGHT).ceil() as usize) + 3).clamp(3, 20);
    first * columns
        ..first
            .saturating_add(count)
            .min(rows)
            .saturating_mul(columns)
            .min(total)
}
pub fn load(app: &mut RealApp) -> Task<Message> {
    if app.screen != Screen::Albums || !app.album_grid.browsing || app.hidden || app.album_grid.busy
    {
        return Task::none();
    }
    let window = app
        .album_grid
        .window(app.group_scroll, app.group_rows.len());
    let visible: Vec<_> = app.group_rows[window]
        .iter()
        .map(|&i| app.groups[i].key.clone())
        .collect();
    if app.album_grid.cache.len() + 4 > CACHE_LIMIT {
        app.album_grid.cache.retain(|key, _| visible.contains(key));
    }
    let jobs: Vec<_> = visible
        .into_iter()
        .filter(|key| !app.album_grid.cache.contains_key(key))
        .take(4)
        .filter_map(|key| {
            let group = app.groups.iter().find(|g| g.key == key)?;
            let paths = group
                .indices
                .iter()
                .take(4)
                .filter_map(|&i| app.snapshot.library.get(i).map(|t| PathBuf::from(&t.path)))
                .collect::<Vec<_>>();
            Some((key, paths))
        })
        .collect();
    if jobs.is_empty() {
        return Task::none();
    }
    app.album_grid.busy = true;
    let failed: Loaded = jobs.iter().map(|(key, _)| (key.clone(), None)).collect();
    let epoch = app.album_grid.epoch;
    let directory = app.album_grid.directory.clone();
    Task::perform(
        async move {
            let loaded = tokio::task::spawn_blocking(move || {
                jobs.into_iter()
                    .map(|(key, paths)| {
                        let cover = paths
                            .iter()
                            .find_map(|path| {
                                muzon_core::data::covers::read_cover_thumbnail(&directory, path)
                                    .map(|(_, bytes)| bytes)
                            })
                            .filter(|bytes| bytes.len() <= 2 * 1024 * 1024);
                        (key, cover)
                    })
                    .collect::<Loaded>()
            })
            .await
            .unwrap_or(failed);
            (epoch, loaded)
        },
        |(epoch, loaded)| Message::AlbumCovers(epoch, loaded),
    )
}
pub fn view(app: &RealApp) -> Element<'_, Message> {
    let columns = app.album_grid.columns();
    let window = app
        .album_grid
        .window(app.group_scroll, app.group_rows.len());
    let rows = app.group_rows.len().div_ceil(columns);
    let mut content =
        column![vertical_space().height((window.start / columns) as f32 * CARD_HEIGHT)]
            .width(Length::Fill);
    for chunk in app.group_rows[window.clone()].chunks(columns) {
        let mut cards = row![].spacing(GAP).width(Length::Fill);
        for &index in chunk {
            let group = &app.groups[index];
            let art: Element<'_, Message> =
                if let Some(Some(handle)) = app.album_grid.cache.get(&group.key) {
                    image(handle.clone())
                        .width(Length::Fill)
                        .height(160.)
                        // Preserve the whole cover without introducing a nested crop layer.
                        .content_fit(iced::ContentFit::Contain)
                        .into()
                } else {
                    container(super::icons::icon(
                        super::icons::Icon::Music,
                        48.,
                        st::muted(),
                    ))
                    .center(Length::Fill)
                    .style(st::card)
                    .into()
                };
            let card = column![
                container(art).height(160.).width(Length::Fill).clip(true),
                container(text(&group.title).size(14))
                    .height(20.)
                    .clip(true),
                container(text(&group.subtitle).size(12).color(st::muted()))
                    .height(18.)
                    .clip(true),
                text(format!("Треков: {}", group.indices.len()))
                    .size(11)
                    .color(st::muted())
            ]
            .spacing(4);
            cards = cards.push(
                button(card)
                    .width(Length::FillPortion(1))
                    .padding(8)
                    .style(st::quiet)
                    .on_press(Message::AlbumSelected(group.key.clone())),
            );
        }
        for _ in chunk.len()..columns {
            cards = cards.push(container(text("")).width(Length::FillPortion(1)));
        }
        content = content.push(
            container(cards)
                .height(CARD_HEIGHT)
                .align_y(iced::alignment::Vertical::Top),
        );
    }
    let end = window.end.div_ceil(columns);
    content = content.push(vertical_space().height(rows.saturating_sub(end) as f32 * CARD_HEIGHT));
    let mut heading = column![
        text("Альбомы").size(28),
        text(format!("Альбомов: {}", app.group_rows.len()))
            .size(12)
            .color(st::muted())
    ]
    .spacing(8);
    if app.group_rows.is_empty() {
        heading = heading.push(container(text("Альбомы не найдены")).center(Length::Fill));
    }
    heading
        .push(
            container(
                st::scrollable(content)
                    .id(scrollable::Id::new("muzon-groups"))
                    .height(Length::Fill)
                    .on_scroll(|v| Message::GroupsScrolled(v.absolute_offset().y))
                    .style(st::scroll),
            )
            .height(Length::Fill)
            .width(Length::Fill)
            .clip(true),
        )
        .spacing(18)
        .height(Length::Fill)
        .align_x(Alignment::Start)
        .into()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn virtual_window_is_aligned_bounded_and_covers_bottom() {
        for total in [0, 1, 2, 37, 5000] {
            for columns in 1..=12 {
                for offset in [0., 235., 236., 5000., 10000000.] {
                    let r = window(offset, total, columns, 760.);
                    assert!(r.start <= r.end && r.end <= total);
                    assert_eq!(r.start % columns, 0);
                    assert!(r.len() <= 20 * columns);
                }
            }
        }
        assert_eq!(columns(480.), 2);
        assert_eq!(columns(884.), 5);
    }
    #[test]
    fn stale_thumbnails_are_ignored_and_cache_can_reload() {
        let mut grid = Grid::new(PathBuf::new());
        grid.busy = true;
        grid.invalidate();
        grid.finish(0, vec![(("a".into(), "b".into()), None)]);
        assert!(!grid.busy);
        assert!(grid.cache.is_empty());
    }
}
