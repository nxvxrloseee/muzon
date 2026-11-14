//! Pure queue bookkeeping for Muzon's future playback core.
//!
//! A faithful port of `src/lib/queueOps.ts` and `src/lib/shuffle.ts`. The
//! queue is three interdependent pieces — the elements, a permutation of
//! indices into them that is the shuffled play order, and the index of the
//! element that is playing. Every mutation has to keep the permutation whole
//! and keep the cursor pointing at the same *element* rather than at the same
//! number — exactly the kind of index arithmetic that belongs outside a store
//! and under test.
//!
//! Deliberately absent: dependencies, Tauri, GStreamer, async, locks, events,
//! file I/O, and the app's `Track` type — [`QueueShape`] is generic and
//! elements are addressed purely by index. The random shuffle
//! (`shuffledIndices`, Fisher–Yates) is not ported either: a permutation is
//! always passed in explicitly, and generating one belongs to a higher layer.
//!
//! Navigation is a pure policy, ported from `queueStore.ts` (`playNext`,
//! `playPrevious`, `peekNextPath`, `displayOrder`): [`next_action`] and
//! [`previous_action`] decide what should play and answer with a
//! [`Navigation`] action, [`peek_next_index`] answers where "next" would land
//! (the gapless arming question), and [`display_order_indices`] answers what
//! the queue view shows. They only read the shape — executing a
//! [`Navigation::Play`] is up to the core. [`play_order_indices`] is the
//! un-rotated active order the Muzon control socket's queue window is
//! addressed in; [`display_order_indices`] is that order rotated around the
//! cursor.
//!
//! Restoring a saved session is a pure mapping plan, ported from
//! `sessionRestore.ts` (`rebuildQueue`): [`plan_restore`] matches the saved
//! paths against the current library, re-indexes the cursor and the shuffle
//! permutation onto whatever survived, and answers with a [`RestorePlan`]
//! the core can turn into a live queue. JSON parsing, file storage,
//! `normalizeSession`, volume and playback position stay out.
//!
//! # Correspondence with the TypeScript original
//!
//! - [`QueueShape::cursor`] is `Option<usize>`; `None` is the TS `-1`
//!   ("nothing playing"), the same value `domain/session.rs` persists as
//!   `cursor: i32`.
//! - TS signals a no-op by returning the *same object*, which callers detect
//!   by identity (`queueStore.ts`'s `shape === before`). Here every operation
//!   hands the shape back: [`remove_at`] reports `removed`, [`move_in_view`]
//!   reports `changed`, and an empty addition to [`append_tracks`] /
//!   [`insert_after_cursor`] returns the shape unchanged.
//! - Indices are `usize`, so TS's negative-index no-op cases cannot occur.
//! - `shuffle_order` is `Vec<usize>` here; `domain/session.rs` persists it as
//!   `Vec<u32>` — convert at that future integration boundary.
//!
//! # Preconditions
//!
//! Operations assume a *valid* shape: `shuffle_order` is a full permutation
//! of `0..queue.len()` — check with [`is_permutation`] — and `cursor` is
//! `None` or a valid index into `queue`. Nothing here validates or repairs
//! its input; data read from disk is checked once, at the restore boundary.
//! On an invalid shape the result of an operation is unspecified. What holds
//! for any input: a no-op hands the shape back untouched, and elements are
//! only ever dropped by a removal that reports `removed: true`.
//!
//! For valid shapes the behavior matches the TypeScript original exactly.
//!
//! ```
//! use muzon_queue::{append_tracks, QueueShape};
//!
//! let shape = QueueShape::new(vec!["a", "b", "c"]);
//! let next = append_tracks(shape, vec!["d"]);
//! assert_eq!(next.queue, ["a", "b", "c", "d"]);
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod navigation;
mod ops;
mod restore;
mod shuffle;

pub use navigation::{
    display_order_indices, next_action, peek_next_index, play_order_indices, previous_action,
    Navigation, RepeatMode,
};
pub use ops::{
    append_tracks, insert_after_cursor, move_in_view, remove_at, MoveResult, RemovalResult,
};
pub use restore::{plan_restore, RestoreError, RestorePlan};
pub use shuffle::{identity_indices, is_permutation, rotate_to_start};

/// The queue's three interdependent pieces: the elements, a permutation of
/// indices into them that is the shuffled play order, and the index of the
/// element that is playing.
///
/// The operations in this crate assume a valid shape: `shuffle_order` is a
/// full permutation of `0..queue.len()` and `cursor` is `None` or a valid
/// index into `queue`. Nothing validates or repairs the input here — data
/// from disk is checked once, at the restore boundary — and the result of an
/// operation on an invalid shape is unspecified.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct QueueShape<T> {
    /// Base element list, in library order — what the queue *is*, as opposed
    /// to the order it plays in.
    pub queue: Vec<T>,
    /// Fixed permutation of indices into `queue`: the play order while
    /// shuffling. Only regenerated for a genuinely new playback context —
    /// toggling shuffle reuses it as-is, so flipping the switch never
    /// reshuffles what's already queued. Must always satisfy
    /// [`is_permutation`] for `queue.len()`.
    pub shuffle_order: Vec<usize>,
    /// Index into `queue` of the currently playing element.
    ///
    /// `None` corresponds to `-1` in the TypeScript original and in
    /// `domain/session.rs` (`cursor: i32`): nothing is playing, and queue
    /// operations fall back to appending. Any `Some(i)` must satisfy
    /// `i < queue.len()`.
    pub cursor: Option<usize>,
}

impl<T> QueueShape<T> {
    /// A shape with the sequential play order and nothing playing.
    pub fn new(queue: Vec<T>) -> Self {
        let shuffle_order = identity_indices(queue.len());
        Self {
            queue,
            shuffle_order,
            cursor: None,
        }
    }
}
