//! The navigation policy: what plays next, what came before, and what the
//! queue view shows — a port of `playNext`, `playPrevious`, `displayOrder`
//! and `peekNextPath` from `src/store/queueStore.ts`.
//!
//! Decisions only, no effects: every function merely *reads* the shape and
//! answers with a [`Navigation`] action or an index that the future playback
//! core will execute. Nothing here moves the cursor, reorders elements, or
//! requires them to be `Clone`.
//!
//! [`RepeatMode`] mirrors the persisted `RepeatMode` of
//! `src-tauri/src/domain/session.rs` one to one.

use crate::shuffle::{identity_indices, rotate_to_start};
use crate::QueueShape;

/// How the player behaves at the ends of the queue, one to one with the
/// `RepeatMode` persisted by `domain/session.rs` (`off` / `one` / `all`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RepeatMode {
    /// End of queue stops playback.
    #[default]
    Off,
    /// The current element repeats forever.
    One,
    /// End of queue wraps around to the first element.
    All,
}

/// A navigation decision the playback core can execute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Navigation {
    /// Nothing to do: an empty queue, or `One` with nothing playing.
    Noop,
    /// Play the element at this index into `queue`. Always a valid index for
    /// a valid shape.
    Play(usize),
    /// The queue ended with repeat off: stop playback.
    Stop,
}

/// The order elements play in this session: the shuffle order when shuffle is
/// on, the queue's own order otherwise — *un-rotated*.
///
/// This is the order the control socket's queue window is addressed in: its
/// `position` values count from the top of the active play order, not from
/// the playing element, so it is deliberately not
/// [`display_order_indices`] — that one is this order rotated around the
/// cursor for the drawer view.
#[must_use]
pub fn play_order_indices<T>(shape: &QueueShape<T>, shuffle: bool) -> Vec<usize> {
    if shuffle {
        shape.shuffle_order.clone()
    } else {
        identity_indices(shape.queue.len())
    }
}

/// What "next" means, ported from `playNext` in `queueStore.ts`.
///
/// The element after the current one *in the active order*; under [`RepeatMode::One`]
/// the current element itself repeats. At the end of the order [`RepeatMode::All`]
/// wraps to its first element and [`RepeatMode::Off`] says stop. An empty queue
/// is a no-op.
///
/// With nothing playing (`cursor` is `None` — the TS `-1`) TS's
/// `order.indexOf(-1)` is `-1`, so `nextPos = 0` and playback starts at the
/// first element of the active order. The one exception is
/// [`RepeatMode::One`]: there is no track to repeat, so — exactly like TS's
/// `playAtQueueIndex(-1)` finding nothing — the decision is a no-op.
#[must_use]
pub fn next_action<T>(shape: &QueueShape<T>, shuffle: bool, repeat: RepeatMode) -> Navigation {
    if shape.queue.is_empty() {
        return Navigation::Noop;
    }

    if repeat == RepeatMode::One {
        return shape.cursor.map_or(Navigation::Noop, Navigation::Play);
    }

    let order = play_order_indices(shape, shuffle);
    // TS: `order.indexOf(cursor) + 1`, where `indexOf(-1)` is -1 when nothing
    // plays — the "next" position then lands on the first element.
    let position = shape
        .cursor
        .and_then(|cursor| order.iter().position(|&i| i == cursor));
    match order.get(position.map_or(0, |p| p + 1)).copied() {
        Some(index) => Navigation::Play(index),
        // Past the end: `All` wraps to the first element of the active order,
        // `Off` stops. (`One` already returned.)
        None => match repeat {
            RepeatMode::All => order
                .first()
                .copied()
                .map_or(Navigation::Noop, Navigation::Play),
            _ => Navigation::Stop,
        },
    }
}

/// What "previous" means, ported from `playPrevious` in `queueStore.ts`.
///
/// The element before the current one *in the active order*, clamped at the
/// front: on the first element "previous" stays on it (TS's
/// `Math.max(0, pos - 1)`), and with nothing playing it starts the first
/// element of the active order (TS's `indexOf(-1)` is `-1`, clamped to 0).
/// A non-empty queue therefore never stops or no-ops here; an empty one does.
#[must_use]
pub fn previous_action<T>(shape: &QueueShape<T>, shuffle: bool) -> Navigation {
    if shape.queue.is_empty() {
        return Navigation::Noop;
    }

    let order = play_order_indices(shape, shuffle);
    let position = shape
        .cursor
        .and_then(|cursor| order.iter().position(|&i| i == cursor));
    let index = position
        .and_then(|p| p.checked_sub(1))
        .and_then(|p| order.get(p).copied())
        .or_else(|| order.first().copied());
    index.map_or(Navigation::Noop, Navigation::Play)
}

/// Where "next" would land, ported from `peekNextPath` in `queueStore.ts`
/// (which arms the gapless backend with the follow-up track).
///
/// [`next_action`]'s answer as a plain index, *except* at the two places
/// where TS itself diverges: repeat off at the end of the queue peeks `None`
/// rather than [`Navigation::Stop`], and with nothing playing there is
/// nothing to peek — `None` — where [`next_action`] would start the first
/// element of the active order.
#[must_use]
pub fn peek_next_index<T>(
    shape: &QueueShape<T>,
    shuffle: bool,
    repeat: RepeatMode,
) -> Option<usize> {
    if shape.queue.is_empty() {
        return None;
    }
    // TS peeks null when nothing plays (`cursor < 0`).
    let cursor = shape.cursor?;

    if repeat == RepeatMode::One {
        return Some(cursor);
    }

    let order = play_order_indices(shape, shuffle);
    // Unreachable `?` for a valid shape: the cursor is always in the order.
    let position = order.iter().position(|&i| i == cursor)?;
    match order.get(position + 1).copied() {
        Some(index) => Some(index),
        None if repeat == RepeatMode::All => order.first().copied(),
        None => None,
    }
}

/// What the queue view shows, ported from `displayOrder` in `queueStore.ts`:
/// the active play order rotated so the playing element comes first, with the
/// rest still to come following in play order.
///
/// With nothing playing (`None`, the TS `-1`) the order is shown as it is.
#[must_use]
pub fn display_order_indices<T>(shape: &QueueShape<T>, shuffle: bool) -> Vec<usize> {
    let order = play_order_indices(shape, shuffle);
    rotate_to_start(&order, shape.cursor)
}
