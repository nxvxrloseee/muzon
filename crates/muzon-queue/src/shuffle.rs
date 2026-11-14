//! Deterministic order helpers, ported from `src/lib/shuffle.ts`.
//!
//! `shuffledIndices` (Fisher–Yates) is intentionally not ported: random
//! shuffling belongs to a higher layer, and every permutation crossing this
//! crate is passed in explicitly.

/// `0..length` in order — the play order when shuffle is off.
#[must_use]
pub fn identity_indices(length: usize) -> Vec<usize> {
    (0..length).collect()
}

/// Rotates `order` so `start` comes first, wrapping the rest around after it —
/// what the queue view renders: the playing element at the top, everything
/// still to come below it.
///
/// `None` (the TS `-1`) and a `start` that is not in `order` both leave the
/// order unchanged, exactly like TS's `indexOf` returning `-1` and the
/// `p <= 0` early return.
#[must_use]
pub fn rotate_to_start(order: &[usize], start: Option<usize>) -> Vec<usize> {
    let Some(start) = start else {
        return order.to_vec();
    };
    match order.iter().position(|&i| i == start) {
        Some(p) if p > 0 => order[p..]
            .iter()
            .chain(order[..p].iter())
            .copied()
            .collect(),
        _ => order.to_vec(),
    }
}

/// True when `order` is a genuine permutation of `0..length`, which is what
/// the queue's shuffle order has to be for `indexOf`-based navigation to reach
/// every element exactly once. Worth checking on anything that came off disk.
///
/// TS also checks `Number.isInteger` and `i >= 0`; a `usize` can be neither
/// fractional nor negative, so only the range and uniqueness checks remain.
#[must_use]
pub fn is_permutation(order: &[usize], length: usize) -> bool {
    if order.len() != length {
        return false;
    }
    let mut seen = vec![false; length];
    for &i in order {
        if i >= length || seen[i] {
            return false;
        }
        seen[i] = true;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_indices_counts_from_zero() {
        assert_eq!(identity_indices(3), [0, 1, 2]);
        assert_eq!(identity_indices(0), Vec::<usize>::new());
    }

    #[test]
    fn rotate_wraps_the_rest_around_after_the_start() {
        assert_eq!(rotate_to_start(&[0, 1, 2], Some(2)), [2, 0, 1]);
        assert_eq!(rotate_to_start(&[3, 1, 2, 0], Some(1)), [1, 2, 0, 3]);
        // The element with this value is already at position 0: unchanged,
        // like TS's `p <= 0` early return.
        assert_eq!(rotate_to_start(&[0, 2, 1], Some(0)), [0, 2, 1]);
    }

    #[test]
    fn rotate_leaves_the_order_alone_when_nothing_is_playing() {
        // TS passes the cursor, which is -1 when nothing plays: indexOf(-1)
        // is -1, and the order comes back as it is.
        assert_eq!(rotate_to_start(&[2, 0, 1], None), [2, 0, 1]);
        // A start that isn't in the order takes the same early return.
        assert_eq!(rotate_to_start(&[0, 1, 2], Some(9)), [0, 1, 2]);
    }

    #[test]
    fn is_permutation_accepts_only_full_index_coverage() {
        assert!(is_permutation(&[], 0));
        assert!(is_permutation(&[1, 0, 2], 3));
        assert!(!is_permutation(&[0, 1], 3)); // wrong length
        assert!(!is_permutation(&[0, 0, 2], 3)); // duplicate
        assert!(!is_permutation(&[0, 1, 3], 3)); // out of range
    }
}
