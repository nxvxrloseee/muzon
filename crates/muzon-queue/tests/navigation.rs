//! The navigation policy against the scenarios in
//! `src/store/queueStore.test.ts`: advancing, previous, peeking and what the
//! view shows — sequential and explicitly shuffled, at the first / middle /
//! last element and with nothing playing, under all three repeat modes.
//! Navigation only reads the shape, so that contract is checked with
//! elements that don't implement `Clone`.

use muzon_queue::{
    display_order_indices, identity_indices, is_permutation, next_action, peek_next_index,
    play_order_indices, previous_action, rotate_to_start, Navigation, QueueShape, RepeatMode,
};

use Navigation::{Noop, Play, Stop};
use RepeatMode::{All, Off, One};

fn shape<'a>(
    queue: &[&'a str],
    shuffle_order: &[usize],
    cursor: Option<usize>,
) -> QueueShape<&'a str> {
    QueueShape {
        queue: queue.to_vec(),
        shuffle_order: shuffle_order.to_vec(),
        cursor,
    }
}

/// queue a, b, c — shuffle off, so the play order is the queue order.
fn sequential(cursor: Option<usize>) -> QueueShape<&'static str> {
    shape(&["a", "b", "c"], &[0, 1, 2], cursor)
}

/// queue a, b, c — shuffle on, play order c, a, b.
fn shuffled(cursor: Option<usize>) -> QueueShape<&'static str> {
    shape(&["a", "b", "c"], &[2, 0, 1], cursor)
}

mod next_action {
    use super::*;

    #[test]
    fn an_empty_queue_is_a_noop_under_every_repeat_mode() {
        let empty = shape(&[], &[], None);
        assert_eq!(next_action(&empty, false, Off), Noop);
        assert_eq!(next_action(&empty, false, One), Noop);
        assert_eq!(next_action(&empty, false, All), Noop);
        assert_eq!(next_action(&empty, true, All), Noop);
    }

    #[test]
    fn advances_in_queue_order_when_shuffle_is_off() {
        assert_eq!(next_action(&sequential(Some(0)), false, Off), Play(1));
        assert_eq!(next_action(&sequential(Some(1)), false, Off), Play(2));
        assert_eq!(next_action(&sequential(Some(1)), false, All), Play(2));
    }

    #[test]
    fn follows_the_shuffle_order_when_shuffle_is_on() {
        // Play order c, a, b: after a (0) comes b (1), after c (2) comes a (0).
        assert_eq!(next_action(&shuffled(Some(0)), true, Off), Play(1));
        assert_eq!(next_action(&shuffled(Some(2)), true, Off), Play(0));
        assert_eq!(next_action(&shuffled(Some(2)), true, All), Play(0));
    }

    #[test]
    fn one_repeats_the_current_element() {
        assert_eq!(next_action(&sequential(Some(1)), false, One), Play(1));
        assert_eq!(next_action(&shuffled(Some(1)), true, One), Play(1));
        assert_eq!(next_action(&shuffled(Some(2)), true, One), Play(2));
    }

    #[test]
    fn stops_at_the_end_when_repeat_is_off() {
        assert_eq!(next_action(&sequential(Some(2)), false, Off), Stop);
        // b (1) is the last element of the play order c, a, b.
        assert_eq!(next_action(&shuffled(Some(1)), true, Off), Stop);
    }

    #[test]
    fn all_wraps_to_the_first_element_of_the_active_order() {
        assert_eq!(next_action(&sequential(Some(2)), false, All), Play(0));
        // After the last element of the play order c, a, b, wrap to its first.
        assert_eq!(next_action(&shuffled(Some(1)), true, All), Play(2));
    }

    #[test]
    fn with_no_cursor_next_starts_the_first_element_of_the_active_order() {
        // TS `order.indexOf(-1) + 1` is 0: playback simply starts from the top.
        assert_eq!(next_action(&sequential(None), false, Off), Play(0));
        assert_eq!(next_action(&sequential(None), false, All), Play(0));
        assert_eq!(next_action(&shuffled(None), true, Off), Play(2));
        assert_eq!(next_action(&shuffled(None), true, All), Play(2));
    }

    #[test]
    fn with_no_cursor_one_has_nothing_to_repeat() {
        // TS `playAtQueueIndex(-1)` finds no track and does nothing.
        assert_eq!(next_action(&sequential(None), false, One), Noop);
        assert_eq!(next_action(&shuffled(None), true, One), Noop);
    }

    #[test]
    fn a_single_element_queue() {
        let single = shape(&["a"], &[0], Some(0));
        assert_eq!(next_action(&single, false, Off), Stop);
        assert_eq!(next_action(&single, false, All), Play(0));
        assert_eq!(next_action(&single, false, One), Play(0));

        let idle = shape(&["a"], &[0], None);
        assert_eq!(next_action(&idle, false, Off), Play(0));
        assert_eq!(next_action(&idle, false, One), Noop);
    }
}

mod previous_action {
    use super::*;

    #[test]
    fn an_empty_queue_is_a_noop() {
        assert_eq!(previous_action(&shape(&[], &[], None), false), Noop);
        assert_eq!(previous_action(&shape(&[], &[], None), true), Noop);
    }

    #[test]
    fn steps_back_in_the_active_order() {
        assert_eq!(previous_action(&sequential(Some(2)), false), Play(1));
        assert_eq!(previous_action(&sequential(Some(1)), false), Play(0));
        // Play order c, a, b: before a (0) is c (2), before b (1) is a (0).
        assert_eq!(previous_action(&shuffled(Some(0)), true), Play(2));
        assert_eq!(previous_action(&shuffled(Some(1)), true), Play(0));
    }

    #[test]
    fn stays_on_the_first_element() {
        assert_eq!(previous_action(&sequential(Some(0)), false), Play(0));
        // c (2) heads the play order c, a, b: "previous" stays on it.
        assert_eq!(previous_action(&shuffled(Some(2)), true), Play(2));
    }

    #[test]
    fn with_no_cursor_it_starts_the_first_element_of_the_active_order() {
        // TS clamps `max(0, indexOf(-1) - 1)` to 0.
        assert_eq!(previous_action(&sequential(None), false), Play(0));
        assert_eq!(previous_action(&shuffled(None), true), Play(2));
    }

    #[test]
    fn a_non_empty_queue_never_stops_or_noops() {
        for cursor in [Some(0), Some(1), Some(2), None] {
            for shuffle in [true, false] {
                let decision = previous_action(&sequential(cursor), shuffle);
                assert_ne!(decision, Noop);
                assert_ne!(decision, Stop);
            }
        }
    }
}

mod peek_next_index {
    use super::*;

    #[test]
    fn an_empty_queue_and_no_cursor_peek_nothing() {
        let empty = shape(&[], &[], None);
        assert_eq!(peek_next_index(&empty, false, Off), None);
        assert_eq!(peek_next_index(&empty, true, All), None);

        // TS `peekNextPath` returns null for `cursor < 0`, in every mode.
        for repeat in [Off, One, All] {
            assert_eq!(peek_next_index(&sequential(None), false, repeat), None);
            assert_eq!(peek_next_index(&shuffled(None), true, repeat), None);
        }
    }

    #[test]
    fn peeks_the_follower_in_the_active_order() {
        assert_eq!(peek_next_index(&sequential(Some(0)), false, Off), Some(1));
        // Play order c, a, b: after c (2) comes a (0).
        assert_eq!(peek_next_index(&shuffled(Some(2)), true, Off), Some(0));
        assert_eq!(peek_next_index(&shuffled(Some(2)), true, All), Some(0));
    }

    #[test]
    fn one_peeks_the_current_element() {
        assert_eq!(peek_next_index(&sequential(Some(1)), false, One), Some(1));
        assert_eq!(peek_next_index(&shuffled(Some(2)), true, One), Some(2));
    }

    #[test]
    fn at_the_end_off_peeks_nothing_and_all_wraps() {
        assert_eq!(peek_next_index(&sequential(Some(2)), false, Off), None);
        assert_eq!(peek_next_index(&sequential(Some(2)), false, All), Some(0));
        // b (1) is the last element of the play order c, a, b.
        assert_eq!(peek_next_index(&shuffled(Some(1)), true, Off), None);
        assert_eq!(peek_next_index(&shuffled(Some(1)), true, All), Some(2));
    }
}

mod display_order_indices {
    use super::*;

    #[test]
    fn an_empty_queue_shows_nothing() {
        assert!(display_order_indices(&shape(&[], &[], None), false).is_empty());
        assert!(display_order_indices(&shape(&[], &[], None), true).is_empty());
    }

    #[test]
    fn shows_the_queue_order_when_shuffle_is_off() {
        assert_eq!(
            display_order_indices(&sequential(Some(0)), false),
            [0, 1, 2]
        );
        assert_eq!(
            display_order_indices(&sequential(Some(1)), false),
            [1, 2, 0]
        );
        assert_eq!(
            display_order_indices(&sequential(Some(2)), false),
            [2, 0, 1]
        );
    }

    #[test]
    fn rotates_the_shuffle_order_around_the_cursor() {
        // Play order c, a, b: the view leads with the playing element and
        // keeps the rest in play order.
        assert_eq!(display_order_indices(&shuffled(Some(2)), true), [2, 0, 1]);
        assert_eq!(display_order_indices(&shuffled(Some(0)), true), [0, 1, 2]);
        assert_eq!(display_order_indices(&shuffled(Some(1)), true), [1, 2, 0]);
    }

    #[test]
    fn shows_the_order_as_is_when_nothing_is_playing() {
        assert_eq!(display_order_indices(&sequential(None), false), [0, 1, 2]);
        assert_eq!(display_order_indices(&shuffled(None), true), [2, 0, 1]);
    }
}

mod play_order_indices {
    use super::*;

    #[test]
    fn is_the_queue_order_when_shuffle_is_off() {
        // The saved permutation is ignored, not rotated: shuffle off means
        // the queue's own order plays.
        assert_eq!(play_order_indices(&shuffled(Some(1)), false), [0, 1, 2]);
        assert_eq!(
            play_order_indices(&shape(&[], &[], None), false),
            Vec::<usize>::new()
        );
    }

    #[test]
    fn is_the_saved_permutation_as_is_when_shuffle_is_on() {
        // NOT rotated around the cursor, even though 1 is playing: this is
        // the order the control socket's window positions count in.
        assert_eq!(play_order_indices(&shuffled(Some(1)), true), [2, 0, 1]);
        assert_eq!(play_order_indices(&shuffled(None), true), [2, 0, 1]);
    }

    #[test]
    fn display_order_is_this_order_rotated_around_the_cursor() {
        for cursor in [Some(0), Some(1), Some(2), None] {
            let s = shuffled(cursor);
            for shuffle in [true, false] {
                assert_eq!(
                    display_order_indices(&s, shuffle),
                    rotate_to_start(&play_order_indices(&s, shuffle), cursor),
                    "cursor {cursor:?}, shuffle {shuffle}"
                );
            }
        }
    }
}

mod read_only {
    use super::*;

    /// An element without `Clone`: navigation must never need to copy any.
    #[derive(Debug, PartialEq)]
    struct Token(usize);

    fn token_shape() -> QueueShape<Token> {
        QueueShape {
            queue: (0..3).map(Token).collect(),
            shuffle_order: vec![2, 0, 1],
            cursor: Some(1),
        }
    }

    #[test]
    fn navigation_leaves_the_shape_untouched_and_needs_no_clone() {
        let shape = token_shape();

        // b (1) is the last element of the play order c, a, b.
        assert_eq!(next_action(&shape, true, Off), Stop);
        assert_eq!(next_action(&shape, true, One), Play(1));
        assert_eq!(next_action(&shape, true, All), Play(2));
        assert_eq!(previous_action(&shape, true), Play(0));
        assert_eq!(peek_next_index(&shape, true, All), Some(2));
        assert_eq!(display_order_indices(&shape, true), [1, 2, 0]);

        // Every element, the cursor and the permutation survived — compared
        // against a freshly built shape, since `Token` cannot be cloned.
        assert_eq!(
            shape,
            QueueShape {
                queue: (0..3).map(Token).collect(),
                shuffle_order: vec![2, 0, 1],
                cursor: Some(1),
            }
        );
        assert_eq!(shape.cursor, Some(1));
        assert_eq!(shape.shuffle_order, [2, 0, 1]);
        assert_eq!(shape.queue, [Token(0), Token(1), Token(2)]);
    }
}

mod systematic {
    use super::*;

    /// A fixed deterministic scramble per length — reversed, then rotated by
    /// one. No RNG anywhere.
    fn scramble(len: usize) -> Vec<usize> {
        let mut order: Vec<usize> = (0..len).rev().collect();
        if !order.is_empty() {
            order.rotate_left(1);
        }
        order
    }

    #[test]
    fn decisions_stay_valid_and_consistent_across_every_shape() {
        for len in 0..=4usize {
            for base_order in [identity_indices(len), scramble(len)] {
                let cursors = std::iter::once(None).chain((0..len).map(Some));
                for cursor in cursors {
                    let shape = QueueShape {
                        queue: (0..len).map(|i| i.to_string()).collect(),
                        shuffle_order: base_order.clone(),
                        cursor,
                    };
                    for shuffle in [true, false] {
                        for repeat in [Off, One, All] {
                            let next = next_action(&shape, shuffle, repeat);
                            match next {
                                Play(index) => {
                                    assert!(
                                        index < len,
                                        "next_action -> Play({index}) out of range for {len}"
                                    );
                                }
                                Noop => assert!(
                                    len == 0 || (repeat == One && cursor.is_none()),
                                    "unexpected Noop (len {len}, cursor {cursor:?}, repeat {repeat:?})"
                                ),
                                Stop => assert!(
                                    len > 0 && repeat == Off && cursor.is_some(),
                                    "unexpected Stop (len {len}, cursor {cursor:?}, repeat {repeat:?})"
                                ),
                            }

                            // While something is playing, the peek and the
                            // decision agree; with nothing playing TS peeks
                            // null where next_action would start the first
                            // element — the deliberate divergence.
                            match cursor {
                                Some(_) => match next {
                                    Play(index) => {
                                        assert_eq!(
                                            peek_next_index(&shape, shuffle, repeat),
                                            Some(index)
                                        );
                                    }
                                    Stop => {
                                        assert_eq!(peek_next_index(&shape, shuffle, repeat), None);
                                    }
                                    Noop => unreachable!("something is playing"),
                                },
                                None => {
                                    assert_eq!(peek_next_index(&shape, shuffle, repeat), None);
                                }
                            }

                            let previous = previous_action(&shape, shuffle);
                            if len == 0 {
                                assert_eq!(previous, Noop);
                            } else {
                                match previous {
                                    Play(index) => assert!(
                                        index < len,
                                        "previous -> Play({index}) out of range for {len}"
                                    ),
                                    other => panic!(
                                        "previous on a non-empty queue never stalls: {other:?}"
                                    ),
                                }
                            }
                        }

                        let display = display_order_indices(&shape, shuffle);
                        assert!(
                            is_permutation(&display, len),
                            "display {display:?} for len {len}"
                        );
                        match cursor {
                            Some(c) => {
                                assert_eq!(display[0], c, "the view leads with the playing element")
                            }
                            None => {
                                let active = if shuffle {
                                    base_order.clone()
                                } else {
                                    identity_indices(len)
                                };
                                assert_eq!(display, active);
                            }
                        }
                    }
                }
            }
        }
    }
}
