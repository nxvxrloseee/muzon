//! View geometry shared by both frontends: the windowed-list arithmetic and
//! the time formatting. Nothing here knows which backend is running — the demo
//! screen and the real player use the same row height and the same window, so
//! the two list implementations behave identically.

use std::ops::Range;

/// Fixed row height in logical pixels — the windowed list's core assumption.
pub const ROW_HEIGHT: f32 = 40.0;
/// Rows built per frame (window size, overscan included) — never the full
/// list, and never proportional to the library size.
pub const WINDOW_ROWS: usize = 64;

pub const MARKER_WIDTH: f32 = 18.0;
pub const POSITION_WIDTH: f32 = 28.0;
pub const TITLE_WIDTH: f32 = 260.0;
pub const ARTIST_WIDTH: f32 = 170.0;
pub const ALBUM_WIDTH: f32 = 210.0;
pub const TIME_WIDTH: f32 = 48.0;

/// The row range to build for this frame: [`WINDOW_ROWS`] rows starting at the
/// scroll position, clamped to the list. The scroll offset arrives in pixels
/// from `scrollable::on_scroll`; one row is [`ROW_HEIGHT`] pixels tall.
pub fn visible_window(scroll_y: f32, total: usize) -> Range<usize> {
    let first = ((scroll_y / ROW_HEIGHT).floor() as usize).min(total);
    first..first.saturating_add(WINDOW_ROWS).min(total)
}

/// `m:ss`, rounded down — the player bar's clock.
pub fn mmss(secs: f64) -> String {
    let secs = if secs.is_finite() { secs.max(0.0) } else { 0.0 };
    let whole = secs as u64;
    format!("{}:{:02}", whole / 60, whole % 60)
}

/// The last lyric line that has started at `position_secs`, or `None` before
/// the first one. Deliberately shared: the demo's unsynced lines and the real
/// player's LRC both highlight through this.
pub fn current_lyric_line(times: &[f64], position_secs: f64) -> Option<usize> {
    let mut current = None;
    for (index, &time) in times.iter().enumerate() {
        if time <= position_secs + 0.05 {
            current = Some(index);
        } else {
            break;
        }
    }
    current
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_covers_the_scroll_position_and_clamps_at_the_end() {
        assert_eq!(visible_window(0.0, 5000), 0..64);
        assert_eq!(visible_window(ROW_HEIGHT * 10.0, 5000), 10..74);
        assert_eq!(visible_window(ROW_HEIGHT * 4990.0, 5000), 4990..5000);
        assert_eq!(visible_window(0.0, 0), 0..0);
    }

    #[test]
    fn clock_formatting_is_stable_including_nonsense_values() {
        assert_eq!(mmss(0.0), "0:00");
        assert_eq!(mmss(61.9), "1:01");
        assert_eq!(mmss(-3.0), "0:00");
        assert_eq!(mmss(f64::NAN), "0:00");
    }

    #[test]
    fn the_current_lyric_line_is_the_last_one_started() {
        let times = [0.0, 1.0, 2.0];
        assert_eq!(current_lyric_line(&times, -1.0), None);
        assert_eq!(current_lyric_line(&times, 1.4), Some(1));
        assert_eq!(current_lyric_line(&times, 60.0), Some(2));
        assert_eq!(current_lyric_line(&[], 5.0), None);
    }
}
