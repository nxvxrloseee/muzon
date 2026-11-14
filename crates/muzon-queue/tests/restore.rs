//! Ports of the `rebuildQueue` scenarios in `src/lib/sessionRestore.test.ts`,
//! plus the cases the restore plan has to survive: deletions before and of
//! the playing track, repeated queue paths, duplicate library paths,
//! impossible cursors, damaged saved orders, and fallback orders that may or
//! may not be usable. Every successful plan is checked to keep the
//! `QueueShape` preconditions: a valid permutation, a cursor in range, and
//! queue elements that are live indices into the library.

use muzon_queue::{identity_indices, is_permutation, plan_restore, RestoreError};

use std::cell::Cell;

const LIBRARY: [&str; 3] = ["/a.mp3", "/b.mp3", "/c.mp3"];

/// Counts how often `fallback_order` ran: at most once per call, and never
/// for an empty result or an order that survived the re-indexing.
#[derive(Default)]
struct FallbackSpy(Cell<usize>);

impl FallbackSpy {
    /// A deterministic replacement: the identity permutation.
    fn order(&self, len: usize) -> Vec<usize> {
        self.0.set(self.0.get() + 1);
        identity_indices(len)
    }

    fn calls(&self) -> usize {
        self.0.get()
    }
}

mod saved_queue_mapping {
    use super::*;

    #[test]
    fn restores_the_queue_in_its_saved_order_not_library_order() {
        let spy = FallbackSpy::default();
        let plan = plan_restore(&["/c.mp3", "/a.mp3", "/b.mp3"], &[], 1, &LIBRARY, |len| {
            spy.order(len)
        })
        .unwrap()
        .unwrap();
        // Queue holds library indices, in saved order: c = 2, a = 0, b = 1.
        assert_eq!(plan.shape.queue, [2, 0, 1]);
        assert_eq!(plan.shape.cursor, Some(1));
        // The TS session leaves the shuffle order empty, which cannot survive
        // as a permutation: redrawn through the fallback.
        assert_eq!(plan.shape.shuffle_order, [0, 1, 2]);
        assert!(plan.shuffle_repaired);
        assert_eq!(spy.calls(), 1);
    }

    #[test]
    fn reindexes_the_cursor_around_tracks_that_left_the_library() {
        let spy = FallbackSpy::default();
        // "/a.mp3" is gone, so what was index 2 is now index 1 - resuming on
        // the saved index instead would come back on the wrong song.
        let plan = plan_restore(
            &["/a.mp3", "/b.mp3", "/c.mp3"],
            &[],
            2,
            &["/b.mp3", "/c.mp3"],
            |len| spy.order(len),
        )
        .unwrap()
        .unwrap();
        assert_eq!(plan.shape.queue, [0, 1]);
        assert_eq!(plan.shape.cursor, Some(1));
        assert!(plan.shuffle_repaired);
        assert_eq!(spy.calls(), 1);
    }

    #[test]
    fn reindexes_the_shuffle_order_around_removed_tracks() {
        let spy = FallbackSpy::default();
        let plan = plan_restore(
            &["/a.mp3", "/b.mp3", "/c.mp3"],
            &[2, 0, 1],
            0,
            &["/a.mp3", "/c.mp3"],
            |len| spy.order(len),
        )
        .unwrap()
        .unwrap();
        // Old 2 -> new 1, old 0 -> new 0, old 1 dropped. The surviving
        // relative order is still a genuine permutation: kept as is.
        assert_eq!(plan.shape.queue, [0, 1]);
        assert_eq!(plan.shape.shuffle_order, [1, 0]);
        assert_eq!(plan.shape.cursor, Some(0));
        assert!(!plan.shuffle_repaired);
        assert_eq!(spy.calls(), 0);
    }

    #[test]
    fn redraws_a_shuffle_order_that_is_no_longer_a_permutation() {
        let spy = FallbackSpy::default();
        let plan = plan_restore(
            &["/a.mp3", "/b.mp3", "/c.mp3"],
            &[0, 0],
            0,
            &LIBRARY,
            |len| spy.order(len),
        )
        .unwrap()
        .unwrap();
        // TS sorts the redrawn order and checks it covers 0..3; with a
        // deterministic fallback the result is known exactly.
        assert_eq!(plan.shape.queue, [0, 1, 2]);
        assert_eq!(plan.shape.shuffle_order, [0, 1, 2]);
        assert!(plan.shuffle_repaired);
        assert_eq!(spy.calls(), 1);
    }

    #[test]
    fn reports_no_cursor_when_the_track_that_was_playing_is_gone() {
        let spy = FallbackSpy::default();
        let plan = plan_restore(&["/a.mp3", "/b.mp3"], &[], 0, &["/b.mp3"], |len| {
            spy.order(len)
        })
        .unwrap()
        .unwrap();
        assert_eq!(plan.shape.queue, [0]);
        assert_eq!(plan.shape.cursor, None);
        assert!(plan.shuffle_repaired);
        assert_eq!(spy.calls(), 1);
    }

    #[test]
    fn returns_none_when_nothing_of_the_saved_queue_survives() {
        let spy = FallbackSpy::default();
        let gone = plan_restore(&["/gone.mp3"], &[0], 0, &LIBRARY, |len| spy.order(len)).unwrap();
        assert_eq!(gone, None);

        let empty = plan_restore(&[], &[], -1, &LIBRARY, |len| spy.order(len)).unwrap();
        assert_eq!(empty, None);

        // Nothing to plan for: the fallback is never consulted.
        assert_eq!(spy.calls(), 0);
    }
}

mod cursor_edges {
    use super::*;

    #[test]
    fn a_deletion_before_the_current_track_keeps_the_same_song_playing() {
        let spy = FallbackSpy::default();
        let library = ["/b.mp3", "/c.mp3"];
        let plan = plan_restore(
            &["/a.mp3", "/b.mp3", "/c.mp3"],
            &[0, 1, 2],
            2,
            &library,
            |len| spy.order(len),
        )
        .unwrap()
        .unwrap();
        let cursor = plan.shape.cursor.unwrap();
        assert_eq!(library[plan.shape.queue[cursor]], "/c.mp3");
        assert!(!plan.shuffle_repaired);
        assert_eq!(spy.calls(), 0);
    }

    #[test]
    fn deleting_the_playing_track_itself_leaves_no_cursor() {
        let spy = FallbackSpy::default();
        let plan = plan_restore(
            &["/a.mp3", "/b.mp3", "/c.mp3"],
            &[0, 1, 2],
            1,
            &["/a.mp3", "/c.mp3"],
            |len| spy.order(len),
        )
        .unwrap()
        .unwrap();
        assert_eq!(plan.shape.queue, [0, 1]);
        assert_eq!(plan.shape.cursor, None);
        assert!(!plan.shuffle_repaired);
        assert_eq!(spy.calls(), 0);
    }

    #[test]
    fn negative_and_out_of_range_cursors_mean_nothing_is_playing() {
        for saved_cursor in [-1, -5, 3, 99] {
            let plan = plan_restore(
                &["/a.mp3", "/b.mp3", "/c.mp3"],
                &[0, 1, 2],
                saved_cursor,
                &LIBRARY,
                identity_indices,
            )
            .unwrap()
            .unwrap();
            assert_eq!(plan.shape.cursor, None, "saved cursor {saved_cursor}");
            assert_eq!(plan.shape.queue, [0, 1, 2]);
            assert_eq!(plan.shape.shuffle_order, [0, 1, 2]);
            assert!(!plan.shuffle_repaired);
        }
    }
}

mod duplicates {
    use super::*;

    #[test]
    fn repeated_queue_paths_keep_their_own_positions() {
        // "/a" twice: every saved occurrence is its own position, so the
        // shuffle order tells them apart by position, never by path.
        let plan = plan_restore(
            &["/a.mp3", "/b.mp3", "/a.mp3"],
            &[2, 1, 0],
            0,
            &LIBRARY,
            identity_indices,
        )
        .unwrap()
        .unwrap();
        assert_eq!(plan.shape.queue, [0, 1, 0]);
        // Old position 2 (the second "/a") -> new position 2, and so on.
        assert_eq!(plan.shape.shuffle_order, [2, 1, 0]);
        assert_eq!(plan.shape.cursor, Some(0));
        assert!(!plan.shuffle_repaired);
    }

    #[test]
    fn the_cursor_can_sit_on_a_later_duplicate() {
        let plan = plan_restore(
            &["/a.mp3", "/a.mp3"],
            &[1, 0],
            1,
            &LIBRARY,
            identity_indices,
        )
        .unwrap()
        .unwrap();
        assert_eq!(plan.shape.queue, [0, 0]);
        assert_eq!(plan.shape.shuffle_order, [1, 0]);
        assert_eq!(plan.shape.cursor, Some(1));
    }

    #[test]
    fn duplicate_library_paths_use_the_last_occurrence() {
        // TS's `new Map(...)` overwrites earlier entries on re-set, so "/x"
        // resolves to index 2, not 0.
        let plan = plan_restore(
            &["/x.mp3", "/y.mp3"],
            &[0, 1],
            0,
            &["/x.mp3", "/y.mp3", "/x.mp3"],
            identity_indices,
        )
        .unwrap()
        .unwrap();
        assert_eq!(plan.shape.queue, [2, 1]);
        assert_eq!(plan.shape.cursor, Some(0));
        assert_eq!(plan.shape.shuffle_order, [0, 1]);
        assert!(!plan.shuffle_repaired);
    }
}

mod damaged_order {
    use super::*;

    #[test]
    fn a_damaged_saved_order_falls_back_exactly_once() {
        let cases: [(&[u32], &str); 4] = [
            (&[0, 0, 2], "a repeated position"),
            (&[0, 2], "a missing position"),
            (&[0, 1, 7], "a position past the end of the saved queue"),
            (&[0, 1, u32::MAX], "a position no usize can name"),
        ];
        for (saved_order, why) in cases {
            let spy = FallbackSpy::default();
            let plan = plan_restore(
                &["/a.mp3", "/b.mp3", "/c.mp3"],
                saved_order,
                0,
                &LIBRARY,
                |len| spy.order(len),
            )
            .unwrap()
            .unwrap();
            assert_eq!(plan.shape.queue, [0, 1, 2]);
            assert_eq!(plan.shape.shuffle_order, [0, 1, 2]);
            assert_eq!(plan.shape.cursor, Some(0));
            assert!(plan.shuffle_repaired, "{why} must be repaired");
            assert_eq!(spy.calls(), 1, "{why} must fall back exactly once");
        }
    }
}

mod fallback {
    use super::*;

    #[test]
    fn a_valid_fallback_order_is_used_as_is() {
        let plan = plan_restore(&["/a.mp3", "/b.mp3"], &[0, 0], 0, &LIBRARY, |len| {
            (0..len).rev().collect()
        })
        .unwrap()
        .unwrap();
        assert_eq!(plan.shape.queue, [0, 1]);
        assert_eq!(plan.shape.shuffle_order, [1, 0]);
        assert_eq!(plan.shape.cursor, Some(0));
        assert!(plan.shuffle_repaired);
    }

    #[test]
    fn an_invalid_fallback_order_is_an_error() {
        let plan = plan_restore(&["/a.mp3", "/b.mp3"], &[0, 0], 0, &LIBRARY, |len| {
            vec![0; len]
        });
        assert_eq!(plan.unwrap_err(), RestoreError::InvalidFallbackOrder);
    }
}

mod systematic {
    use super::*;

    #[test]
    fn every_success_is_a_valid_shape_with_live_library_indices() {
        let library_paths = ["/a", "/b", "/c"];
        let in_library = |path: &str| library_paths.contains(&path);

        // Every subset of the library as the saved queue, optionally plus a
        // path that left it, against intact and damaged saved orders and
        // every cursor around the range.
        for mask in 0..8usize {
            for with_gone in [false, true] {
                let mut saved_paths: Vec<&str> = (0..3)
                    .filter(|i| mask & (1 << i) != 0)
                    .map(|i| library_paths[i])
                    .collect();
                if with_gone {
                    saved_paths.push("/gone");
                }
                let saved_len = saved_paths.len();

                let mut orders: Vec<Vec<u32>> = vec![
                    (0..saved_len as u32).collect(),
                    (0..saved_len as u32).rev().collect(),
                    Vec::new(),
                    vec![0; saved_len],
                    (0..saved_len as u32).filter(|p| *p != 1).collect(),
                ];
                let mut out_of_range = (0..saved_len as u32).collect::<Vec<u32>>();
                out_of_range.push(saved_len as u32 + 7);
                out_of_range.push(u32::MAX);
                orders.push(out_of_range);

                for saved_order in &orders {
                    for saved_cursor in -2..=(saved_len as i32 + 2) {
                        let spy = FallbackSpy::default();
                        let plan = plan_restore(
                            &saved_paths,
                            saved_order,
                            saved_cursor,
                            &library_paths,
                            |len| spy.order(len),
                        )
                        .unwrap();

                        let Some(plan) = plan else {
                            // Nothing survives: no fallback, no library path
                            // left in the saved queue.
                            assert_eq!(spy.calls(), 0);
                            assert!(!saved_paths.iter().any(|&p| in_library(p)));
                            continue;
                        };

                        // The restored queue is the surviving saved paths,
                        // in saved order, as library indices.
                        let surviving: Vec<usize> = saved_paths
                            .iter()
                            .filter(|&&p| in_library(p))
                            .map(|&p| library_paths.iter().position(|&l| l == p).unwrap())
                            .collect();
                        assert_eq!(plan.shape.queue, surviving);
                        assert!(
                            plan.shape
                                .queue
                                .iter()
                                .all(|&index| index < library_paths.len()),
                            "queue {:?} leaves the library",
                            plan.shape.queue
                        );

                        // A successful plan always carries a valid
                        // permutation and a cursor in range.
                        assert!(is_permutation(
                            &plan.shape.shuffle_order,
                            plan.shape.queue.len()
                        ));
                        if let Some(cursor) = plan.shape.cursor {
                            assert!(cursor < plan.shape.queue.len());
                        }

                        // The cursor still names the song it used to — or
                        // nothing, exactly when that song is gone or was
                        // never addressed.
                        let expected = if saved_cursor >= 0
                            && (saved_cursor as usize) < saved_len
                            && in_library(saved_paths[saved_cursor as usize])
                        {
                            Some(saved_paths[saved_cursor as usize])
                        } else {
                            None
                        };
                        match (plan.shape.cursor, expected) {
                            (Some(cursor), Some(path)) => {
                                assert_eq!(library_paths[plan.shape.queue[cursor]], path);
                            }
                            (None, None) => {}
                            other => {
                                panic!("cursor mismatch: {other:?} for saved_cursor {saved_cursor}")
                            }
                        }

                        // The identity fallback is always valid, so the plan
                        // is repaired exactly when the fallback ran.
                        assert_eq!(plan.shuffle_repaired, spy.calls() == 1);
                    }
                }
            }
        }
    }
}
