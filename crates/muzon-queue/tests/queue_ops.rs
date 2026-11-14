//! Ports of the scenarios in `src/lib/queueOps.test.ts`, plus the invariant
//! checks the port has to satisfy: the shuffle order stays a full permutation,
//! the cursor keeps naming the same element through changes around it, and
//! identical element values never affect addressing (everything is by index).
//! The no-op paths hand the shape back untouched — checked with elements that
//! don't implement `Clone`.

use muzon_queue::{
    append_tracks, identity_indices, insert_after_cursor, is_permutation, move_in_view, remove_at,
    rotate_to_start, QueueShape,
};

/// What the queue drawer actually shows: the play order, rotated so the
/// playing track is first. Assertions read better against this than against
/// the index arrays behind them.
fn view_of<'a>(shape: &QueueShape<&'a str>, shuffle: bool) -> Vec<&'a str> {
    let order = if shuffle {
        shape.shuffle_order.clone()
    } else {
        identity_indices(shape.queue.len())
    };
    rotate_to_start(&order, shape.cursor)
        .into_iter()
        .map(|i| shape.queue[i])
        .collect()
}

/// The invariant every operation has to preserve: the shuffle order stays a
/// permutation of the queue, so navigation reaches every track exactly once,
/// and the cursor stays in range.
fn expect_consistent<T>(shape: &QueueShape<T>) {
    assert!(
        is_permutation(&shape.shuffle_order, shape.queue.len()),
        "shuffle_order {:?} is not a permutation of {} element(s)",
        shape.shuffle_order,
        shape.queue.len()
    );
    if let Some(cursor) = shape.cursor {
        assert!(
            cursor < shape.queue.len(),
            "cursor {cursor} is out of range for {} element(s)",
            shape.queue.len()
        );
    }
}

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

fn abc() -> QueueShape<&'static str> {
    shape(&["a", "b", "c"], &[2, 0, 1], Some(0))
}

#[test]
fn new_shape_is_sequential_and_idle() {
    let s = QueueShape::new(vec!["a", "b"]);
    assert_eq!(s.shuffle_order, [0, 1]);
    assert_eq!(s.cursor, None);
}

#[test]
fn operations_on_an_empty_queue_are_no_ops_or_append() {
    let empty = shape(&[], &[], None);

    let removed = remove_at(empty.clone(), 0, false);
    assert!(!removed.removed);
    assert_eq!(removed.shape, empty);

    let moved = move_in_view(empty.clone(), false, 0, 0);
    assert!(!moved.changed);
    assert_eq!(moved.shape, empty);

    assert_eq!(append_tracks(empty, vec!["a"]), shape(&["a"], &[0], None));
}

mod append_tracks {
    use super::*;

    #[test]
    fn adds_to_the_end_of_both_the_queue_and_the_play_order() {
        let next = append_tracks(abc(), vec!["d"]);
        assert_eq!(next.queue, ["a", "b", "c", "d"]);
        assert_eq!(next.shuffle_order, [2, 0, 1, 3]);
        assert_eq!(next.cursor, Some(0));
        expect_consistent(&next);
    }

    #[test]
    fn is_a_no_op_for_an_empty_addition() {
        let before = abc();
        assert_eq!(append_tracks(before.clone(), vec![]), before);
    }

    #[test]
    fn works_on_an_empty_queue() {
        let next = append_tracks(shape(&[], &[], None), vec!["a", "b"]);
        assert_eq!(next.queue, ["a", "b"]);
        assert_eq!(next.shuffle_order, [0, 1]);
        assert_eq!(next.cursor, None);
        expect_consistent(&next);
    }
}

mod insert_after_cursor {
    use super::*;

    #[test]
    fn puts_the_track_next_in_both_orders_at_once() {
        // Playing "a" (index 0); shuffle order is c, a, b.
        let next = insert_after_cursor(abc(), vec!["x"]);
        assert_eq!(next.queue, ["a", "x", "b", "c"]);
        // "a" is still what's playing...
        assert_eq!(next.queue[next.cursor.unwrap()], "a");
        // ...and "x" follows it in both readings of "next".
        assert_eq!(next.queue[next.cursor.unwrap() + 1], "x");
        let cursor = next.cursor.unwrap();
        let order_pos = next
            .shuffle_order
            .iter()
            .position(|&i| i == cursor)
            .unwrap();
        assert_eq!(next.queue[next.shuffle_order[order_pos + 1]], "x");
        expect_consistent(&next);
    }

    #[test]
    fn keeps_the_cursor_on_the_same_track_when_the_insert_lands_after_it() {
        // Playing "c" (index 2): inserting after it shifts nothing below it.
        let next = insert_after_cursor(shape(&["a", "b", "c"], &[0, 1, 2], Some(2)), vec!["x"]);
        assert_eq!(next.queue[next.cursor.unwrap()], "c");
        expect_consistent(&next);
    }

    #[test]
    fn falls_back_to_appending_when_nothing_is_playing() {
        let next = insert_after_cursor(shape(&["a"], &[0], None), vec!["x"]);
        assert_eq!(next.queue, ["a", "x"]);
        assert_eq!(next.cursor, None);
        expect_consistent(&next);
    }
}

mod remove_at {
    use super::*;

    #[test]
    fn shifts_the_cursor_down_when_something_before_it_goes() {
        let result = remove_at(shape(&["a", "b", "c"], &[0, 1, 2], Some(2)), 0, false);
        assert!(result.removed);
        assert!(!result.was_current);
        assert_eq!(result.shape.queue, ["b", "c"]);
        assert_eq!(result.shape.queue[result.shape.cursor.unwrap()], "c");
        expect_consistent(&result.shape);
    }

    #[test]
    fn leaves_the_cursor_alone_when_something_after_it_goes() {
        let result = remove_at(shape(&["a", "b", "c"], &[0, 1, 2], Some(0)), 2, false);
        assert!(result.removed);
        assert_eq!(result.shape.queue[result.shape.cursor.unwrap()], "a");
        expect_consistent(&result.shape);
    }

    #[test]
    fn reports_where_to_continue_when_the_playing_track_is_removed() {
        let result = remove_at(shape(&["a", "b", "c"], &[0, 1, 2], Some(0)), 0, false);
        assert!(result.removed);
        assert!(result.was_current);
        let next_cursor = result.next_cursor.unwrap();
        assert_eq!(result.shape.queue[next_cursor], "b");
        assert_eq!(result.shape.cursor, result.next_cursor);
        expect_consistent(&result.shape);
    }

    #[test]
    fn follows_the_shuffle_order_not_the_queue_order_when_shuffled() {
        // Shuffle order c, a, b - playing "c", so "a" is what comes next.
        let result = remove_at(shape(&["a", "b", "c"], &[2, 0, 1], Some(2)), 2, true);
        assert!(result.removed);
        assert_eq!(result.shape.queue[result.next_cursor.unwrap()], "a");
        expect_consistent(&result.shape);
    }

    #[test]
    fn reports_no_continuation_when_the_last_track_in_the_order_is_removed() {
        let result = remove_at(shape(&["a", "b"], &[0, 1], Some(1)), 1, false);
        assert!(result.removed);
        assert!(result.was_current);
        assert_eq!(result.next_cursor, None);
        assert_eq!(result.shape.cursor, None);
        expect_consistent(&result.shape);
    }

    #[test]
    fn empties_cleanly() {
        let result = remove_at(shape(&["a"], &[0], Some(0)), 0, false);
        assert!(result.removed);
        assert!(result.shape.queue.is_empty());
        assert_eq!(result.next_cursor, None);
        expect_consistent(&result.shape);
    }

    #[test]
    fn ignores_an_out_of_range_index() {
        let before = abc();
        let result = remove_at(before.clone(), 9, false);
        assert!(!result.removed);
        assert!(!result.was_current);
        assert_eq!(result.next_cursor, None);
        assert_eq!(result.shape, before);
        // TS also passes -1 here; `usize` has no negative index, so only the
        // far side of the range can be exercised.
    }
}

mod move_in_view {
    use super::*;

    #[test]
    fn reorders_what_the_view_shows_with_shuffle_off() {
        // Playing "a", so the view already reads a, b, c.
        let before = shape(&["a", "b", "c"], &[0, 1, 2], Some(0));
        let next = move_in_view(before, false, 2, 1);
        assert!(next.changed);
        assert_eq!(view_of(&next.shape, false), ["a", "c", "b"]);
        assert_eq!(next.shape.queue[next.shape.cursor.unwrap()], "a");
        expect_consistent(&next.shape);
    }

    #[test]
    fn addresses_positions_in_the_rotated_view_not_in_the_queue() {
        // Playing "c" (queue index 2), so the view reads c, a, b - dragging
        // view position 2 ("b") to position 1 must put "b" ahead of "a", even
        // though in queue terms "b" sits before both.
        let next = move_in_view(shape(&["a", "b", "c"], &[0, 1, 2], Some(2)), false, 2, 1);
        assert!(next.changed);
        assert_eq!(view_of(&next.shape, false), ["c", "b", "a"]);
        assert_eq!(next.shape.queue[next.shape.cursor.unwrap()], "c");
        expect_consistent(&next.shape);
    }

    #[test]
    fn survives_a_drag_across_the_point_where_the_view_wraps() {
        // Playing "d": the view is d, e, a, b, c, so view position 4 ("c")
        // sits before the wrap in the underlying order and position 1 ("e")
        // after it.
        let before = shape(&["a", "b", "c", "d", "e"], &[0, 1, 2, 3, 4], Some(3));
        assert_eq!(view_of(&before, false), ["d", "e", "a", "b", "c"]);
        let next = move_in_view(before, false, 4, 1);
        assert!(next.changed);
        assert_eq!(view_of(&next.shape, false), ["d", "c", "e", "a", "b"]);
        assert_eq!(next.shape.queue[next.shape.cursor.unwrap()], "d");
        expect_consistent(&next.shape);
    }

    #[test]
    fn leaves_the_queue_alone_and_moves_only_the_permutation_when_shuffled() {
        // Shuffle order c, a, b and playing "c", so the view reads c, a, b.
        let before = shape(&["a", "b", "c"], &[2, 0, 1], Some(2));
        let next = move_in_view(before, true, 2, 1);
        assert!(next.changed);
        assert_eq!(next.shape.queue, ["a", "b", "c"]);
        assert_eq!(view_of(&next.shape, true), ["c", "b", "a"]);
        expect_consistent(&next.shape);
    }

    #[test]
    fn keeps_the_queue_playable_when_nothing_is_playing_yet() {
        let next = move_in_view(shape(&["a", "b", "c"], &[0, 1, 2], None), false, 0, 2);
        assert!(next.changed);
        assert_eq!(next.shape.queue, ["b", "c", "a"]);
        assert_eq!(next.shape.cursor, None);
        expect_consistent(&next.shape);
    }

    #[test]
    fn is_a_no_op_for_a_move_that_goes_nowhere_or_out_of_range() {
        let before = abc();

        let result = move_in_view(before.clone(), false, 1, 1);
        assert!(!result.changed);
        assert_eq!(result.shape, before);

        let result = move_in_view(before.clone(), false, 0, 9);
        assert!(!result.changed);
        assert_eq!(result.shape, before);
        // TS also passes -1 as a position; `usize` cannot express that side.
    }
}

mod invariants {
    use super::*;

    #[test]
    fn shuffle_order_stays_a_full_permutation_through_every_operation() {
        // A fixed deterministic scramble per length - reversed, then rotated
        // by one. No RNG anywhere.
        fn scramble(len: usize) -> Vec<usize> {
            let mut order: Vec<usize> = (0..len).rev().collect();
            if !order.is_empty() {
                order.rotate_left(1);
            }
            order
        }

        for len in 0..=5usize {
            for base_order in [identity_indices(len), scramble(len)] {
                let cursors = std::iter::once(None).chain((0..len).map(Some));
                for cursor in cursors {
                    let start = QueueShape {
                        queue: (0..len).map(|i| i.to_string()).collect(),
                        shuffle_order: base_order.clone(),
                        cursor,
                    };
                    expect_consistent(&start);

                    expect_consistent(&append_tracks(start.clone(), vec!["z".to_string()]));
                    expect_consistent(&insert_after_cursor(start.clone(), vec!["z".to_string()]));

                    for index in 0..len {
                        let removed = remove_at(start.clone(), index, true);
                        expect_consistent(&removed.shape);
                        let removed = remove_at(start.clone(), index, false);
                        expect_consistent(&removed.shape);
                    }
                    assert!(!remove_at(start.clone(), len, false).removed);

                    for from in 0..len {
                        for to in 0..len {
                            if from == to {
                                continue;
                            }
                            let moved = move_in_view(start.clone(), true, from, to);
                            expect_consistent(&moved.shape);
                            let moved = move_in_view(start.clone(), false, from, to);
                            expect_consistent(&moved.shape);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn cursor_keeps_the_same_element_when_changes_happen_around_it() {
        let queue: Vec<String> = (0..5).map(|i| format!("t{i}")).collect();
        let orders = [identity_indices(5), vec![3, 0, 4, 1, 2]];

        for order in &orders {
            for cursor in 0..5usize {
                let start = QueueShape {
                    queue: queue.clone(),
                    shuffle_order: order.clone(),
                    cursor: Some(cursor),
                };
                let playing = queue[cursor].clone();

                let after = append_tracks(start.clone(), vec!["z".to_string()]);
                assert_eq!(after.queue[after.cursor.unwrap()], playing);

                let after = insert_after_cursor(start.clone(), vec!["z".to_string()]);
                assert_eq!(after.queue[after.cursor.unwrap()], playing);

                // Removing the playing element moves the cursor by design;
                // everything else must leave the element under it alone.
                for index in 0..5usize {
                    if index == cursor {
                        continue;
                    }
                    for shuffle in [true, false] {
                        let after = remove_at(start.clone(), index, shuffle);
                        assert_eq!(
                            after.shape.queue[after.shape.cursor.unwrap()],
                            playing,
                            "remove {index} (shuffle {shuffle}) lost the playing element"
                        );
                    }
                }

                for from in 0..5usize {
                    for to in 0..5usize {
                        if from == to {
                            continue;
                        }
                        for shuffle in [true, false] {
                            let after = move_in_view(start.clone(), shuffle, from, to);
                            assert_eq!(
                                after.shape.queue[after.shape.cursor.unwrap()],
                                playing,
                                "move {from}->{to} (shuffle {shuffle}) lost the playing element"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn identical_elements_do_not_break_index_addressing() {
        // Two "x" elements and the cursor on the second one.
        let start = shape(&["x", "x", "y"], &[0, 1, 2], Some(1));

        // Removing the FIRST "x" is not removing the playing one, even though
        // the values are equal: addressing is by index.
        let after = remove_at(start.clone(), 0, false);
        assert!(after.removed);
        assert!(!after.was_current);
        assert_eq!(after.shape.cursor, Some(0));
        assert_eq!(after.shape.queue[0], "x");
        expect_consistent(&after.shape);

        // Removing index 1 *is* removing the playing one, even though
        // queue[0] still holds the same value "x".
        let after = remove_at(start.clone(), 1, false);
        assert!(after.removed);
        assert!(after.was_current);
        assert_eq!(after.shape.cursor, after.next_cursor);
        assert_eq!(after.shape.queue, ["x", "y"]);
        expect_consistent(&after.shape);

        // An insert between two equal elements is expressible only by index:
        // the new element must follow the playing one, not the first "x".
        let after = insert_after_cursor(shape(&["x", "x"], &[0, 1], Some(0)), vec!["y"]);
        assert_eq!(after.queue, ["x", "y", "x"]);
        assert_eq!(after.queue[after.cursor.unwrap() + 1], "y");
        expect_consistent(&after);

        // A drag with duplicates moves the permutation, not values: the
        // queue and the cursor are untouched.
        let after = move_in_view(shape(&["x", "x", "y"], &[0, 1, 2], Some(1)), true, 2, 1);
        assert_eq!(after.shape.queue, ["x", "x", "y"]);
        assert_eq!(after.shape.cursor, Some(1));
        assert_eq!(after.shape.shuffle_order, [1, 0, 2]);
        expect_consistent(&after.shape);
    }
}

mod no_clone_elements {
    use super::*;

    /// An element without `Clone`: the no-op paths must hand every element
    /// back by value rather than needing to copy any.
    #[derive(Debug, PartialEq)]
    struct Token(usize);

    fn token_queue() -> QueueShape<Token> {
        QueueShape {
            queue: (0..3).map(Token).collect(),
            shuffle_order: vec![2, 0, 1],
            cursor: Some(1),
        }
    }

    #[test]
    fn an_out_of_range_removal_hands_every_element_back() {
        let expected = token_queue();
        let result = remove_at(token_queue(), 9, false);
        assert!(!result.removed);
        assert!(!result.was_current);
        assert_eq!(result.next_cursor, None);
        assert_eq!(result.shape, expected);
        // The contents, cursor and permutation all survived, without cloning.
        assert_eq!(result.shape.queue, expected.queue);
        assert_eq!(result.shape.cursor, expected.cursor);
        assert_eq!(result.shape.shuffle_order, expected.shuffle_order);
    }

    #[test]
    fn a_move_onto_the_same_position_hands_every_element_back() {
        let expected = token_queue();
        let result = move_in_view(token_queue(), true, 2, 2);
        assert!(!result.changed);
        assert_eq!(result.shape, expected);
        assert_eq!(result.shape.queue, expected.queue);
        assert_eq!(result.shape.cursor, expected.cursor);
        assert_eq!(result.shape.shuffle_order, expected.shuffle_order);
    }

    #[test]
    fn a_move_out_of_range_hands_every_element_back() {
        let expected = token_queue();
        let result = move_in_view(token_queue(), false, 0, 3);
        assert!(!result.changed);
        assert_eq!(result.shape, expected);
        assert_eq!(result.shape.queue, expected.queue);
        assert_eq!(result.shape.cursor, expected.cursor);
        assert_eq!(result.shape.shuffle_order, expected.shuffle_order);
    }
}
