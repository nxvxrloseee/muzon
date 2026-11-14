//! Smart lists contain indices into the immutable library snapshot.
use muzon_core::domain::Track;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Smart {
    MostPlayed,
    RecentlyPlayed,
    Forgotten,
    NeverPlayed,
    RecentlyAdded,
}
impl Smart {
    pub const ALL: [Self; 5] = [
        Self::MostPlayed,
        Self::RecentlyPlayed,
        Self::Forgotten,
        Self::NeverPlayed,
        Self::RecentlyAdded,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Self::MostPlayed => "Часто слушаю",
            Self::RecentlyPlayed => "Недавно слушал",
            Self::Forgotten => "Давно не слушал",
            Self::NeverPlayed => "Ни разу не слушал",
            Self::RecentlyAdded => "Добавлено за месяц",
        }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::MostPlayed => "100 самых прослушиваемых треков",
            Self::RecentlyPlayed => "100 последних прослушанных треков",
            Self::Forgotten => "Слушали хотя бы 3 раза, но не включали больше 60 дней",
            Self::NeverPlayed => "Треки без засчитанных прослушиваний, новые первыми",
            Self::RecentlyAdded => "Добавлено в библиотеку за последние 30 дней",
        }
    }
}
pub fn select(library: &[Track], kind: Smart, now: f64) -> Vec<usize> {
    let day = 86400.;
    let mut indices: Vec<usize> = library
        .iter()
        .enumerate()
        .filter(|(_, t)| match kind {
            Smart::MostPlayed => t.play_count > 0,
            Smart::RecentlyPlayed => t.last_played_at.is_some_and(f64::is_finite),
            Smart::Forgotten => {
                t.play_count >= 3
                    && t.last_played_at
                        .is_some_and(|last| last.is_finite() && now - last > 60. * day)
            }
            Smart::NeverPlayed => t.play_count == 0,
            Smart::RecentlyAdded => t.added_at.is_finite() && now - t.added_at <= 30. * day,
        })
        .map(|(i, _)| i)
        .collect();
    indices.sort_by(|&a, &b| {
        let a = &library[a];
        let b = &library[b];
        match kind {
            Smart::MostPlayed | Smart::Forgotten => b.play_count.cmp(&a.play_count),
            Smart::RecentlyPlayed => b
                .last_played_at
                .unwrap_or(0.)
                .total_cmp(&a.last_played_at.unwrap_or(0.)),
            Smart::NeverPlayed | Smart::RecentlyAdded => b.added_at.total_cmp(&a.added_at),
        }
        .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
    });
    if matches!(
        kind,
        Smart::MostPlayed | Smart::RecentlyPlayed | Smart::Forgotten
    ) {
        indices.truncate(100);
    }
    indices
}
