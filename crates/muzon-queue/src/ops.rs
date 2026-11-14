//! The queue mutations, ported one-to-one from `src/lib/queueOps.ts`.

use std::collections::HashMap;

use crate::shuffle::{identity_indices, rotate_to_start};
use crate::QueueShape;

/// Inserts `tracks` at `queue_index` in the queue and at `order_position` in
/// the shuffle order, shifting every index that the insertion pushed along.
///
/// Private in TS as well. Splice positions are clamped to the length so an
/// out-of-range position appends rather than panics; anything else an invalid
/// shape produces is unspecified (see the preconditions on [`QueueShape`]).
fn insert<T>(
    mut shape: QueueShape<T>,
    tracks: Vec<T>,
    queue_index: usize,
    order_position: usize,
) -> QueueShape<T> {
    if tracks.is_empty() {
        return shape;
    }

    let track_count = tracks.len();
    let shift = |i: usize| if i >= queue_index { i + track_count } else { i };
    // The new elements' indices in queue terms, read before anything moves.
    let inserted: Vec<usize> = (queue_index..queue_index + track_count).collect();

    // Splice into the play order: TS `shifted.splice(orderPosition, 0, …inserted)`,
    // where every old index has already been shifted.
    let mut shifted: Vec<usize> = shape.shuffle_order.iter().map(|&i| shift(i)).collect();
    let tail = shifted.split_off(order_position.min(shifted.len()));
    shifted.extend(inserted);
    shifted.extend(tail);
    shape.shuffle_order = shifted;

    // Splice into the queue: TS `queue.splice(queueIndex, 0, …tracks)`.
    let tail = shape.queue.split_off(queue_index.min(shape.queue.len()));
    shape.queue.extend(tracks);
    shape.queue.extend(tail);

    shape.cursor = shape.cursor.map(shift);
    shape
}

/// Appends to the end of the queue and to the end of the play order. A no-op
/// for an empty addition: the shape comes back unchanged.
pub fn append_tracks<T>(shape: QueueShape<T>, tracks: Vec<T>) -> QueueShape<T> {
    let queue_index = shape.queue.len();
    let order_position = shape.shuffle_order.len();
    insert(shape, tracks, queue_index, order_position)
}

/// Inserts so the tracks play immediately after the current one. Patches both
/// the queue position and the shuffle position rather than picking one, so
/// "play next" means the same thing whether shuffle is on or off. Falls back
/// to appending when nothing is playing (`cursor` is `None` — the TS `-1`).
///
/// A no-op for an empty addition.
pub fn insert_after_cursor<T>(shape: QueueShape<T>, tracks: Vec<T>) -> QueueShape<T> {
    let Some(cursor) = shape.cursor else {
        return append_tracks(shape, tracks);
    };
    // Unreachable fallback for a valid shape: a permutation always holds the
    // cursor.
    let order_position = shape
        .shuffle_order
        .iter()
        .position(|&i| i == cursor)
        .map_or(0, |p| p + 1);
    insert(shape, tracks, cursor + 1, order_position)
}

/// What [`remove_at`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovalResult<T> {
    /// The queue after the call: the input unchanged when nothing was
    /// removed.
    pub shape: QueueShape<T>,
    /// True when the element at `queue_index` existed and was dropped.
    pub removed: bool,
    /// True when the removed element was the one playing, so the caller has to
    /// do something about the audio rather than only the list. Always false
    /// when `removed` is false.
    pub was_current: bool,
    /// Where playback should continue, as an index into the *new* queue:
    /// whatever followed the removed element in the active order, or `None`
    /// (TS `-1`) when it was the last one. Only meaningful together with
    /// `was_current`.
    pub next_cursor: Option<usize>,
}

/// Drops the element at `queue_index`.
///
/// Always hands a shape back: when the index is out of range the input comes
/// back untouched with `removed: false` — no element is lost. (TS returns the
/// *same* shape object there, which the store detects by identity,
/// `queueStore.ts`'s `shape === before`; TS's `queueIndex < 0` case cannot be
/// expressed with `usize`.)
///
/// Assumes a valid shape (see the preconditions on [`QueueShape`]); on an
/// invalid one the result is unspecified.
pub fn remove_at<T>(
    mut shape: QueueShape<T>,
    queue_index: usize,
    shuffle: bool,
) -> RemovalResult<T> {
    if queue_index >= shape.queue.len() {
        return RemovalResult {
            shape,
            removed: false,
            was_current: false,
            next_cursor: None,
        };
    }

    let was_current = shape.cursor == Some(queue_index);
    // When the playing element itself goes, read its follower *before* the
    // removal, while the old indices still hold. For a valid shape the
    // position always exists.
    let following_before = if was_current {
        let order = if shuffle {
            shape.shuffle_order.clone()
        } else {
            identity_indices(shape.queue.len())
        };
        order
            .iter()
            .position(|&i| i == queue_index)
            .and_then(|p| order.get(p + 1).copied())
    } else {
        None
    };

    let shift = |i: usize| if i > queue_index { i - 1 } else { i };
    let next_cursor = following_before.map(shift);

    shape.queue.remove(queue_index);
    shape.shuffle_order = shape
        .shuffle_order
        .into_iter()
        .filter(|&i| i != queue_index)
        .map(shift)
        .collect();
    shape.cursor = if was_current {
        next_cursor
    } else {
        shape.cursor.map(shift)
    };

    RemovalResult {
        shape,
        removed: true,
        was_current,
        next_cursor,
    }
}

/// What [`move_in_view`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveResult<T> {
    /// The queue after the call: the input unchanged when the move was a
    /// no-op.
    pub shape: QueueShape<T>,
    /// True when the move was applied; false when it went nowhere or out of
    /// range.
    pub changed: bool,
}

/// Applies a drag made in the queue view, addressed by position *in that
/// view* — which shows the play order rotated so the playing element comes
/// first.
///
/// That rotation is why this cannot simply move `order[from_view]` to
/// `order[to_view]`: a drag can cross the point where the view wraps back
/// round to the start of the order, and the two positions then no longer mean
/// the same thing. So the move is applied to the sequence as displayed and
/// that sequence becomes the new order outright — which it may, because the
/// playing element heads it either way.
///
/// Always hands a shape back: a move that goes nowhere or out of range returns
/// the input untouched with `changed: false`. (TS returns the *same* shape
/// object there; `queueStore.ts` does not identity-check this one — it re-arms
/// the backend either way.)
///
/// Assumes a valid shape (see the preconditions on [`QueueShape`]); on an
/// invalid one the result is unspecified.
pub fn move_in_view<T>(
    shape: QueueShape<T>,
    shuffle: bool,
    from_view: usize,
    to_view: usize,
) -> MoveResult<T> {
    let length = shape.queue.len();
    if from_view == to_view || from_view >= length || to_view >= length {
        return MoveResult {
            shape,
            changed: false,
        };
    }

    // The sequence the drag is applied to, rotated so the playing element
    // heads it; `None` (nothing playing) leaves the order as it is, like TS.
    let mut moved = if shuffle {
        debug_assert_eq!(shape.shuffle_order.len(), length);
        rotate_to_start(&shape.shuffle_order, shape.cursor)
    } else {
        let identity = identity_indices(length);
        rotate_to_start(&identity, shape.cursor)
    };
    let entry = moved.remove(from_view);
    // TS splices into the already-shortened array: after the removal,
    // `to_view` addresses the sequence *without* the dragged entry.
    moved.insert(to_view, entry);

    if shuffle {
        // The queue itself is untouched, and the cursor keeps naming the same
        // index into it — only the permutation changed.
        return MoveResult {
            shape: QueueShape {
                shuffle_order: moved,
                ..shape
            },
            changed: true,
        };
    }

    // With shuffle off the play order *is* the queue, so the queue itself has
    // to be rewritten — and every index into it re-pointed at the element it
    // named. For a valid shape `moved` is a permutation and every lookup
    // hits; the `unwrap_or` is only there to stay panic-free.
    let new_index_of: HashMap<usize, usize> = moved
        .iter()
        .enumerate()
        .map(|(new_index, &old_index)| (old_index, new_index))
        .collect();
    let remap = |i: usize| new_index_of.get(&i).copied().unwrap_or(i);

    let mut slots: Vec<_> = shape.queue.into_iter().map(Some).collect();
    let queue = moved
        .iter()
        .map(|&old_index| {
            slots[old_index]
                .take()
                .expect("moved is a permutation of the queue")
        })
        .collect();

    MoveResult {
        shape: QueueShape {
            queue,
            shuffle_order: shape.shuffle_order.iter().map(|&i| remap(i)).collect(),
            cursor: shape.cursor.map(remap),
        },
        changed: true,
    }
}
