//! The restore plan: matching a saved queue's paths against the current
//! library — a port of `rebuildQueue` from `src/lib/sessionRestore.ts`.
//!
//! Only the mapping plan is here. Deliberately absent: JSON parsing and file
//! storage, `normalizeSession` (filling in fields an older build omitted),
//! volume and playback position — those stay at the session boundary. No
//! randomness either: TS redraws a broken shuffle order with `shuffledIndices`
//! (Fisher–Yates); here the caller supplies the replacement order, and the app
//! passes its generator at the integration boundary.
//!
//! Saved-field correspondence — `src-tauri/src/domain/session.rs` persists
//! the queue as `queue_paths: Vec<String>`, `shuffle_order: Vec<u32>` and
//! `cursor: i32` (-1 = nothing playing); that is exactly the
//! `&[&str]` / `&[u32]` / `i32` trio [`plan_restore`] takes.

use std::collections::HashMap;

use crate::shuffle::is_permutation;
use crate::QueueShape;

/// The mapping plan [`plan_restore`] hands back for the core to execute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestorePlan {
    /// The restored queue: elements are indices into `library_paths`, in the
    /// saved order minus whatever left the library. `cursor` and
    /// `shuffle_order` address positions within this restored queue, exactly
    /// as in [`QueueShape`].
    pub shape: QueueShape<usize>,
    /// True when the saved shuffle permutation did not survive the mapping
    /// and `fallback_order` replaced it. A repaired plan still satisfies
    /// every [`QueueShape`] precondition — the permutation is valid — but
    /// relaunching mid-shuffle continues in a freshly drawn order rather
    /// than the saved one.
    pub shuffle_repaired: bool,
}

/// Why a restore plan could not be produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreError {
    /// The caller-provided replacement order was not a valid permutation of
    /// the restored queue. TS has no such failure (its `shuffledIndices`
    /// cannot produce one); Rust makes the hand-off explicit.
    InvalidFallbackOrder,
}

/// Matches a saved session's queue against the current library.
///
/// Paths that are no longer in the library are dropped, every surviving
/// occurrence keeps its own position (duplicates stay duplicates), and the
/// cursor and shuffle permutation are re-indexed through the old-to-new
/// position mapping — never by path, which would collapse repeated tracks
/// into one. Without that re-indexing a single deleted track would shift the
/// whole queue by one and resume on the wrong song.
///
/// - `saved_paths` — the saved queue, in saved order (`Session::queue_paths`).
/// - `saved_order` — the saved shuffle permutation, positions within the
///   saved queue (`Session::shuffle_order`).
/// - `saved_cursor` — position in the saved queue, `-1` when nothing was
///   playing (`Session::cursor`).
/// - `library_paths` — the current library; queue elements are indices into
///   it.
/// - `fallback_order` — replacement permutation for a damaged order, called
///   at most once with the restored length. Never called when nothing
///   survives or when the re-indexed saved order is still a valid
///   permutation.
///
/// Returns `Ok(None)` when nothing of the saved queue survives — the TS
/// `null`. The cursor is `None` when it was negative, out of range, or
/// pointed at a track that left. If a library path appears more than once,
/// the **last** occurrence wins, exactly like TS's `new Map(...)` overwriting
/// earlier entries on re-`set` — the later library row is the one a re-scan
/// left in place.
pub fn plan_restore(
    saved_paths: &[&str],
    saved_order: &[u32],
    saved_cursor: i32,
    library_paths: &[&str],
    fallback_order: impl FnOnce(usize) -> Vec<usize>,
) -> Result<Option<RestorePlan>, RestoreError> {
    // TS `new Map(tracks.map((t) => [t.path, t]))`: a duplicate path
    // overwrites the earlier entry, so the last occurrence wins.
    let library_index: HashMap<&str, usize> = library_paths
        .iter()
        .enumerate()
        .map(|(index, &path)| (path, index))
        .collect();

    // Walk the saved queue in saved order; each surviving occurrence becomes
    // a new position, and the old-to-new map remembers where it went.
    let mut new_index_of: HashMap<usize, usize> = HashMap::with_capacity(saved_paths.len());
    let mut queue: Vec<usize> = Vec::with_capacity(saved_paths.len());
    for (old, path) in saved_paths.iter().enumerate() {
        if let Some(&library) = library_index.get(path) {
            new_index_of.insert(old, queue.len());
            queue.push(library);
        }
    }

    if queue.is_empty() {
        return Ok(None);
    }

    // The saved cursor names a position in the *saved* queue; re-index it
    // onto the survivors. Negative (`try_from` fails), out of range, or
    // pointing at a track that left — all mean nothing is playing.
    let cursor = usize::try_from(saved_cursor)
        .ok()
        .and_then(|old| new_index_of.get(&old).copied());

    // The saved permutation re-indexed through old -> new; entries whose old
    // position did not survive (track removed, out of range) drop out. A
    // genuinely saved permutation of a queue that only *lost* tracks stays a
    // valid permutation here; duplicates, gaps and out-of-range indices land
    // in the fallback below, like TS's redraw.
    let mut shuffle_repaired = false;
    let mut shuffle_order: Vec<usize> = saved_order
        .iter()
        .filter_map(|&old| {
            usize::try_from(old)
                .ok()
                .and_then(|old| new_index_of.get(&old).copied())
        })
        .collect();

    if !is_permutation(&shuffle_order, queue.len()) {
        shuffle_order = fallback_order(queue.len());
        if !is_permutation(&shuffle_order, queue.len()) {
            return Err(RestoreError::InvalidFallbackOrder);
        }
        shuffle_repaired = true;
    }

    Ok(Some(RestorePlan {
        shape: QueueShape {
            queue,
            shuffle_order,
            cursor,
        },
        shuffle_repaired,
    }))
}
