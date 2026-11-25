//! UI messages: the only way the view talks to the state.

use crate::demo::state::Screen;

/// One user or timer event.
#[derive(Debug, Clone)]
pub enum Message {
    /// The one-second playback timer fired. The subscription for this exists
    /// only while the prototype is "playing".
    Tick,
    /// The search box text changed. Never mutates the queue.
    SearchChanged(String),
    /// A row of the filtered library list was clicked: the payload is the row
    /// index in the *filtered* list, not the library index.
    TrackRowClicked(usize),
    /// A row of the queue view was clicked: the payload is the position in
    /// the display order that `muzon-queue` produced.
    QueueRowClicked(usize),
    /// Play/pause — prototype state only, there is no sound anywhere.
    TogglePlay,
    /// Skip forward: `muzon-queue` decides where.
    Next,
    /// Skip backward: `muzon-queue` decides where.
    Previous,
    /// Flip the shuffle switch. The saved permutation is reused as-is.
    ToggleShuffle,
    /// Cycle the repeat mode: off → all → one → off.
    CycleRepeat,
    /// Switch the main screen.
    ScreenSelected(Screen),
    /// The library list scrolled, in pixels.
    LibraryScrolled(f32),
    /// The queue list scrolled, in pixels.
    QueueScrolled(f32),
}
