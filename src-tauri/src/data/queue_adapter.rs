//! A borrowed bridge between the persisted [`Session`] and the pure
//! `muzon-queue` shape: it sanitizes the snapshot that came from disk so the
//! shared algorithms can trust it, without owning any of it.
//!
//! This is a safe read of the *current* session, not a session restore: paths
//! are deliberately not matched against the library — the control socket
//! shows queue entries whose files have vanished — and nothing random
//! replaces a damaged permutation, only the queue's own order does.

use crate::domain::session::Session;
use muzon_queue::{identity_indices, is_permutation, QueueShape};

/// The session snapshot as a [`QueueShape`] over borrowed paths, in the order
/// the queue holds them.
///
/// - The cursor: `-1` (nothing playing) and anything below zero become
///   `None`, as does a value past the end of the queue. That is
///   `muzon-queue`'s `None` ↔ `-1` correspondence, enforced once here instead
///   of being re-derived at every call site.
/// - `shuffle_order` goes through `usize::try_from` (no `as` cast) and a full
///   [`is_permutation`] check; a permutation that fails — wrong length,
///   repeats, gaps, out-of-range entries — is replaced with
///   [`identity_indices`] rather than trusted.
///
/// The elements are `&str` borrowed from the session, so nothing is cloned.
pub fn queue_shape(session: &Session) -> QueueShape<&str> {
    let n = session.queue_paths.len();
    let shuffle_order = match session
        .shuffle_order
        .iter()
        .map(|&i| usize::try_from(i))
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(order) if is_permutation(&order, n) => order,
        _ => identity_indices(n),
    };
    let cursor = usize::try_from(session.cursor)
        .ok()
        .filter(|&cursor| cursor < n);
    QueueShape {
        queue: session.queue_paths.iter().map(String::as_str).collect(),
        shuffle_order,
        cursor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(queue_paths: &[&str], cursor: i32, shuffle_order: Vec<u32>) -> Session {
        Session {
            queue_paths: queue_paths.iter().map(|p| (*p).to_string()).collect(),
            shuffle: !shuffle_order.is_empty(),
            shuffle_order,
            cursor,
            ..Session::default()
        }
    }

    #[test]
    fn paths_are_borrowed_not_copied() {
        let s = session(&["/a.flac", "/b.flac"], 0, vec![1, 0]);
        let shape = queue_shape(&s);
        assert_eq!(shape.queue, ["/a.flac", "/b.flac"]);
        // The elements are the session's own strings, not fresh allocations.
        assert_eq!(shape.queue[0].as_ptr(), s.queue_paths[0].as_ptr());
        assert_eq!(shape.queue[1].as_ptr(), s.queue_paths[1].as_ptr());
    }

    #[test]
    fn nothing_playing_and_out_of_range_cursors_become_none() {
        for cursor in [-1, -7, 3, 99] {
            let s = session(&["/a", "/b", "/c"], cursor, vec![0, 1, 2]);
            assert_eq!(queue_shape(&s).cursor, None, "cursor {cursor}");
        }
        let s = session(&["/a"], 0, vec![0]);
        assert_eq!(queue_shape(&s).cursor, Some(0));
    }

    #[test]
    fn a_valid_permutation_passes_through_unchanged() {
        let s = session(&["/a", "/b", "/c"], 1, vec![2, 0, 1]);
        let shape = queue_shape(&s);
        assert_eq!(shape.shuffle_order, [2, 0, 1]);
        assert_eq!(shape.queue, ["/a", "/b", "/c"]);
    }

    #[test]
    fn a_damaged_permutation_becomes_the_queue_order() {
        for damaged in [
            vec![0, 0, 2],        // repeated position
            vec![0, 1],           // wrong length
            vec![0, 1, 9],        // past the end
            vec![0, 1, u32::MAX], // no usize can name it
        ] {
            let why = format!("{damaged:?}");
            let s = session(&["/a", "/b", "/c"], 0, damaged);
            let shape = queue_shape(&s);
            assert_eq!(shape.shuffle_order, [0, 1, 2], "damaged {why}");
        }
    }

    #[test]
    fn queue_paths_are_not_filtered_against_any_library() {
        // The socket shows entries whose files have left the library; the
        // adapter must not second-guess that.
        let s = session(&["/a.flac", "/gone.flac"], -1, vec![]);
        let shape = queue_shape(&s);
        assert_eq!(shape.queue, ["/a.flac", "/gone.flac"]);
        assert_eq!(shape.shuffle_order, [0, 1]);
        assert_eq!(shape.cursor, None);
    }
}
